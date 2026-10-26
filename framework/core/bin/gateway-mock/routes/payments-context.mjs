// Mirrors companion managed-apps/domain/services/payments-context.ts; tested by source conformance.
/** Read-only scoped protocol; never authorizes provider financial mutations. */
import { createHmac, timingSafeEqual } from "node:crypto";
export class PaymentContextError extends Error {
    constructor() {
        super("Payment context is unavailable or no longer matches");
    }
}
function deny() {
    throw new PaymentContextError();
}
const id = (v, prefix) => typeof v === "string" &&
    v.length <= 128 &&
    new RegExp(`^${prefix}[A-Za-z0-9_]+$`).test(v);
const uuid = (v) => typeof v === "string" &&
    /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(v);
const apiVersion = (v) => typeof v === "string" && /^\d{4}-\d{2}-\d{2}(?:\.[a-z]+)?$/.test(v);
/** One immutable credential/base configuration, shared safely across requests. */
export class PaymentsContextProtocol {
    forward;
    now;
    signingKey;
    constructor(secret, base, forward, now = Date.now) {
        this.forward = forward;
        this.now = now;
        if (!secret.trim() || secret.length > 4096)
            deny();
        let url;
        try {
            url = new URL(base);
        }
        catch {
            deny();
        }
        if (!["http:", "https:"].includes(url.protocol) ||
            url.username ||
            url.password ||
            url.search ||
            url.hash)
            deny();
        const normalized = url.href.replace(/\/+$/, "");
        this.clock();
        this.signingKey = createHmac("sha256", secret)
            .update(`forklaunch/payments-context/key/v1\0${normalized}`)
            .digest();
    }
    clock() {
        const value = this.now();
        if (!Number.isSafeInteger(value) ||
            value < 0 ||
            value > Number.MAX_SAFE_INTEGER - 5000)
            return deny();
        return value;
    }
    mac(payload) {
        return createHmac("sha256", this.signingKey)
            .update(`forklaunch/payments-context/token/v1\0${payload}`)
            .digest();
    }
    fresh(scope) {
        const now = this.clock();
        if (!Number.isSafeInteger(scope.issuedAt) ||
            !Number.isSafeInteger(scope.expiresAt) ||
            scope.expiresAt - scope.issuedAt !== 5000 ||
            now < scope.issuedAt ||
            now >= scope.expiresAt)
            deny();
    }
    async same(scope, read) {
        this.fresh(scope);
        const row = await read();
        this.fresh(scope);
        if (!row ||
            row.instanceId !== scope.instanceId ||
            row.organizationId !== scope.organizationId ||
            row.stripeAccountId !== scope.accountId ||
            row.bindingGeneration !== scope.bindingGeneration)
            deny();
    }
    async get(scope, path) {
        this.fresh(scope);
        const result = await this.forward({
            method: "GET",
            path,
            query: "",
            stripeAccount: scope.accountId,
            stripeVersion: scope.apiVersion,
            scopedReadTimeoutMs: Math.max(1, Math.floor(scope.expiresAt - this.clock())),
        });
        this.fresh(scope);
        if (result.status < 200 ||
            result.status >= 300 ||
            Buffer.byteLength(result.body, "utf8") > 1024 * 1024)
            deny();
        let value;
        try {
            value = JSON.parse(result.body);
        }
        catch {
            return deny();
        }
        if (!value || typeof value !== "object" || Array.isArray(value))
            return deny();
        return value;
    }
    async mode(scope) {
        const balance = await this.get(scope, "/v1/balance");
        if (balance.object !== "balance" || balance.livemode !== scope.livemode)
            deny();
    }
    async issue(owner, request, read) {
        const started = this.clock();
        if (!id(request.accountId, "acct_") ||
            typeof request.livemode !== "boolean" ||
            !apiVersion(request.apiVersion))
            return deny();
        const row = await read();
        if (!row ||
            !uuid(row.bindingGeneration) ||
            row.instanceId !== owner.id ||
            row.organizationId !== owner.organizationId ||
            row.stripeAccountId !== request.accountId)
            return deny();
        const scope = {
            version: 1,
            instanceId: owner.id,
            organizationId: owner.organizationId,
            accountId: row.stripeAccountId,
            livemode: request.livemode,
            bindingGeneration: row.bindingGeneration,
            apiVersion: request.apiVersion,
            issuedAt: started,
            expiresAt: started + 5000,
        };
        const account = await this.get(scope, `/v1/accounts/${scope.accountId}`);
        if (account.object !== "account" || account.id !== scope.accountId)
            deny();
        await this.mode(scope);
        await this.same(scope, read);
        const payload = Buffer.from(JSON.stringify(scope)).toString("base64url");
        return {
            scope,
            contextToken: `${payload}.${this.mac(payload).toString("base64url")}`,
        };
    }
    checkout(owner, request, read) {
        return this.scopedRead("checkout", owner, request, read);
    }
    paymentLink(owner, request, read) {
        return this.scopedRead("payment-link", owner, request, read);
    }
    /** Fixed paired evidence read; never exposes an arbitrary event/proxy route. */
    async pausedSubscription(owner, request, read) {
        if (!id(request.eventId, "evt_") || !id(request.subscriptionId, "sub_"))
            return deny();
        const scope = this.readScope(owner, request);
        await this.same(scope, read);
        await this.mode(scope);
        await this.same(scope, read);
        const event = await this.get(scope, `/v1/events/${request.eventId}`);
        await this.same(scope, read);
        if (event.id !== request.eventId ||
            event.object !== "event" ||
            event.type !== "customer.subscription.paused" ||
            event.livemode !== scope.livemode ||
            (event.account != null && event.account !== scope.accountId) ||
            event.context != null ||
            !Number.isSafeInteger(event.created) ||
            event.created < 0)
            return deny();
        const data = event.data;
        const prior = data?.object;
        const before = this.pausedCustomer(prior, request.subscriptionId, scope);
        const subscription = await this.get(scope, `/v1/subscriptions/${request.subscriptionId}`);
        await this.same(scope, read);
        if (this.pausedCustomer(subscription, request.subscriptionId, scope) !==
            before)
            return deny();
        const result = { scope, event, subscription };
        if (Buffer.byteLength(JSON.stringify(result), "utf8") > 1024 * 1024)
            return deny();
        this.fresh(scope);
        return result;
    }
    pausedCustomer(value, subscriptionId, scope) {
        if (!value || typeof value !== "object" || Array.isArray(value))
            return deny();
        const object = value;
        if (object.id !== subscriptionId ||
            object.object !== "subscription" ||
            object.status !== "paused" ||
            object.livemode !== scope.livemode)
            return deny();
        if (id(object.customer, "cus_"))
            return object.customer;
        const customer = object.customer;
        if (!customer ||
            typeof customer !== "object" ||
            Array.isArray(customer) ||
            customer.object !== "customer" ||
            !id(customer.id, "cus_") ||
            (Object.hasOwn(customer, "deleted") && customer.deleted !== false) ||
            (Object.hasOwn(customer, "livemode") &&
                customer.livemode !== scope.livemode))
            return deny();
        return customer.id;
    }
    readScope(owner, request) {
        if (typeof request.contextToken !== "string" ||
            request.contextToken.length > 4096)
            return deny();
        const parts = request.contextToken.split(".");
        if (parts.length !== 2 || !parts.every((p) => /^[A-Za-z0-9_-]+$/.test(p)))
            return deny();
        const [payload, signature] = parts;
        const actual = Buffer.from(signature, "base64url");
        const expected = this.mac(payload);
        if (actual.length !== expected.length ||
            !timingSafeEqual(actual, expected) ||
            actual.toString("base64url") !== signature)
            return deny();
        let scope;
        try {
            scope = JSON.parse(Buffer.from(payload, "base64url").toString("utf8"));
        }
        catch {
            return deny();
        }
        if (!scope ||
            scope.version !== 1 ||
            scope.instanceId !== owner.id ||
            scope.organizationId !== owner.organizationId ||
            !id(scope.accountId, "acct_") ||
            !uuid(scope.bindingGeneration) ||
            typeof scope.livemode !== "boolean" ||
            !apiVersion(scope.apiVersion) ||
            scope.apiVersion !== request.apiVersion)
            return deny();
        return scope;
    }
    async scopedRead(kind, owner, request, read) {
        if (typeof request.contextToken !== "string" ||
            request.contextToken.length > 4096 ||
            !id(request.id, kind === "checkout" ? "cs_" : "plink_"))
            return deny();
        const scope = this.readScope(owner, request);
        await this.same(scope, read);
        await this.mode(scope);
        const object = await this.get(scope, kind === "checkout"
            ? `/v1/checkout/sessions/${request.id}`
            : `/v1/payment_links/${request.id}`);
        if (object.id !== request.id ||
            object.object !==
                (kind === "checkout" ? "checkout.session" : "payment_link") ||
            object.livemode !== scope.livemode)
            deny();
        await this.same(scope, read);
        return { scope, object };
    }
}
