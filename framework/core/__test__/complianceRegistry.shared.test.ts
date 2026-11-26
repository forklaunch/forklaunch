import { describe, expect, it, vi } from "vitest";

/**
 * The package is bundled per entry point, so `persistence` and `services`
 * each load their own copy of the compliance registries. Loading the module
 * twice here stands in for that: a policy registered through one copy must
 * be visible through the other (the retention job read an empty copy and
 * enforced nothing, 2026-11-20).
 */
describe("compliance registries are shared across module copies", () => {
	it("sees retention, user-id field and field compliance registered by another copy", async () => {
		vi.resetModules();
		const writer = await import("../src/persistence/complianceTypes");
		vi.resetModules();
		const reader = await import("../src/persistence/complianceTypes");
		expect(reader).not.toBe(writer);

		writer.registerEntityRetention("SharedRegistryProbe", {
			duration: "P1Y",
			action: "delete",
		} as never);
		writer.registerEntityUserIdField("SharedRegistryProbe", "ownerId");
		writer.registerEntityCompliance(
			"SharedRegistryProbe",
			new Map([["email", "pii"]]) as never,
		);

		expect(reader.getAllRetentionPolicies().has("SharedRegistryProbe")).toBe(
			true,
		);
		expect(reader.getEntityUserIdField("SharedRegistryProbe")).toBe("ownerId");
		expect(reader.getComplianceMetadata("SharedRegistryProbe", "email")).toBe(
			"pii",
		);
	});
});
