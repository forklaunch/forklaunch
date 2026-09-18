/**
 * Postgres-backed scaffold store — same mimic of the ForkLaunch ecommerce
 * module's behavior as store.ts (SQLite), but on a real local Postgres DB so
 * the demo runs on the same engine production would. This is a stand-in that
 * MIRRORS the module's intended contract (catalog / cart / order state machine /
 * inventory) — it is NOT the module's own code. The module's real code runs
 * separately (catalog + cart only, pending PRs 6/7/8).
 */
import { SQL } from 'bun';
import type { NormalizedCatalog } from './model.ts';

const DEFAULT_SEED_STOCK = 100;
const ORDER_TRANSITIONS: Record<string, string[]> = {
  pending: ['paid', 'cancelled'], paid: ['fulfilled', 'cancelled'],
  fulfilled: ['shipped', 'cancelled'], shipped: ['delivered'], delivered: [], cancelled: [],
};

export class PgStore {
  sql: SQL;
  constructor(url: string) { this.sql = new SQL(url); }

  async init() {
    const sql = this.sql;
    await sql`CREATE TABLE IF NOT EXISTS products (
      id SERIAL PRIMARY KEY, external_id TEXT UNIQUE NOT NULL, handle TEXT NOT NULL, source_url TEXT,
      title TEXT NOT NULL, description_html TEXT, vendor TEXT, product_type TEXT, tags JSONB, options JSONB, image_src TEXT)`;
    await sql`CREATE TABLE IF NOT EXISTS variants (
      id SERIAL PRIMARY KEY, product_id INT NOT NULL REFERENCES products(id) ON DELETE CASCADE,
      external_id TEXT UNIQUE NOT NULL, sku TEXT, title TEXT, option_values JSONB,
      price_cents INT NOT NULL, compare_at_price_cents INT, requires_shipping BOOLEAN DEFAULT true)`;
    await sql`CREATE TABLE IF NOT EXISTS inventory (variant_id INT PRIMARY KEY REFERENCES variants(id) ON DELETE CASCADE, stock INT NOT NULL DEFAULT 0)`;
    await sql`CREATE TABLE IF NOT EXISTS carts (id SERIAL PRIMARY KEY, status TEXT NOT NULL DEFAULT 'open')`;
    await sql`CREATE TABLE IF NOT EXISTS cart_items (id SERIAL PRIMARY KEY, cart_id INT NOT NULL REFERENCES carts(id) ON DELETE CASCADE, variant_id INT NOT NULL REFERENCES variants(id), quantity INT NOT NULL)`;
    await sql`CREATE TABLE IF NOT EXISTS orders (id SERIAL PRIMARY KEY, status TEXT NOT NULL DEFAULT 'pending', subtotal_cents INT NOT NULL, tax_cents INT NOT NULL DEFAULT 0, total_cents INT NOT NULL, created_at TIMESTAMPTZ NOT NULL DEFAULT now())`;
    await sql`CREATE TABLE IF NOT EXISTS order_items (id SERIAL PRIMARY KEY, order_id INT NOT NULL REFERENCES orders(id) ON DELETE CASCADE, variant_id INT NOT NULL REFERENCES variants(id), quantity INT NOT NULL, unit_price_cents INT NOT NULL)`;
    return this;
  }

  async importCatalog(cat: NormalizedCatalog): Promise<{ products: number; variants: number }> {
    let pc = 0, vc = 0;
    for (const p of cat.products) {
      const [prow] = await this.sql`
        INSERT INTO products (external_id, handle, source_url, title, description_html, vendor, product_type, tags, options, image_src)
        VALUES (${p.externalId}, ${p.handle}, ${p.sourceUrl ?? ''}, ${p.title}, ${p.descriptionHtml ?? ''}, ${p.vendor ?? ''}, ${p.productType ?? ''}, ${JSON.stringify(p.tags ?? [])}, ${JSON.stringify(p.options ?? [])}, ${p.images?.[0]?.src ?? ''})
        ON CONFLICT (external_id) DO UPDATE SET title=EXCLUDED.title, handle=EXCLUDED.handle RETURNING id`;
      pc++;
      for (const v of p.variants) {
        const [vrow] = await this.sql`
          INSERT INTO variants (product_id, external_id, sku, title, option_values, price_cents, compare_at_price_cents, requires_shipping)
          VALUES (${prow.id}, ${v.externalId}, ${v.sku ?? ''}, ${v.title}, ${JSON.stringify(v.optionValues ?? {})}, ${v.priceCents}, ${v.compareAtPriceCents ?? null}, ${v.requiresShipping ?? true})
          ON CONFLICT (external_id) DO UPDATE SET price_cents=EXCLUDED.price_cents RETURNING id`;
        // Seed the REAL on-hand count when an Admin pull provided it; only fall
        // back to the placeholder for a public pull (inventoryQuantity null).
        const stock = v.inventoryQuantity != null ? v.inventoryQuantity : DEFAULT_SEED_STOCK;
        await this.sql`INSERT INTO inventory (variant_id, stock) VALUES (${vrow.id}, ${stock}) ON CONFLICT (variant_id) DO NOTHING`;
        vc++;
      }
    }
    return { products: pc, variants: vc };
  }

  async getProductByHandle(handle: string) {
    const [p] = await this.sql`SELECT * FROM products WHERE handle=${handle}`;
    if (!p) return null;
    p.variants = await this.sql`SELECT v.*, i.stock FROM variants v JOIN inventory i ON i.variant_id=v.id WHERE v.product_id=${p.id}`;
    return p;
  }
  async listProducts() { return this.sql`SELECT id, handle, title, vendor, product_type, image_src FROM products ORDER BY title`; }
  async variantIdByExternal(ext: string): Promise<number | null> {
    const [v] = await this.sql`SELECT id FROM variants WHERE external_id=${String(ext)}`;
    if (v) return v.id;
    const [p] = await this.sql`SELECT id FROM products WHERE external_id=${String(ext)}`;
    if (p) { const [v2] = await this.sql`SELECT id FROM variants WHERE product_id=${p.id} LIMIT 1`; return v2?.id ?? null; }
    return null;
  }
  async variantExternalById(id: number): Promise<string> {
    const [r] = await this.sql`SELECT external_id FROM variants WHERE id=${id}`; return r?.external_id ?? String(id);
  }
  async createCart(): Promise<number> { const [r] = await this.sql`INSERT INTO carts DEFAULT VALUES RETURNING id`; return r.id; }
  async addToCart(cartId: number, variantId: number, qty: number) { await this.sql`INSERT INTO cart_items (cart_id, variant_id, quantity) VALUES (${cartId}, ${variantId}, ${qty})`; }
  async getCart(cartId: number) {
    const items = await this.sql`
      SELECT ci.id, ci.quantity, v.id AS variant_id, v.title AS variant_title, v.price_cents, p.title AS product_title, p.handle, i.stock
      FROM cart_items ci JOIN variants v ON v.id=ci.variant_id JOIN products p ON p.id=v.product_id JOIN inventory i ON i.variant_id=v.id
      WHERE ci.cart_id=${cartId}`;
    const subtotal = items.reduce((n: number, it: any) => n + it.price_cents * it.quantity, 0);
    return { id: cartId, items, subtotalCents: subtotal };
  }
  async checkout(cartId: number, taxRate = 0.08): Promise<{ orderId: number }> {
    const cart = await this.getCart(cartId);
    if (!cart.items.length) throw new Error('cart is empty');
    for (const it of cart.items) if (it.stock < it.quantity) throw new Error(`out of stock: ${it.product_title} / ${it.variant_title}`);
    const subtotal = cart.subtotalCents, tax = Math.round(subtotal * taxRate), total = subtotal + tax;
    // One transaction: an order with half its lines, or a converted cart with
    // no order, is worse than no order at all.
    return this.sql.begin(async (tx: any) => {
      const [order] = await tx`INSERT INTO orders (status, subtotal_cents, tax_cents, total_cents) VALUES ('pending', ${subtotal}, ${tax}, ${total}) RETURNING id`;
      for (const it of cart.items) await tx`INSERT INTO order_items (order_id, variant_id, quantity, unit_price_cents) VALUES (${order.id}, ${it.variant_id}, ${it.quantity}, ${it.price_cents})`;
      await tx`UPDATE carts SET status='converted' WHERE id=${cartId}`;
      return { orderId: order.id };
    });
  }
  async pay(orderId: number) { await this.transition(orderId, 'paid'); }
  async transition(orderId: number, to: string) {
    // The status change and the inventory it implies land together or not at all.
    await this.sql.begin(async (tx: any) => {
      const [o] = await tx`SELECT status FROM orders WHERE id=${orderId} FOR UPDATE`;
      if (!o) throw new Error('order not found');
      if (!(ORDER_TRANSITIONS[o.status] ?? []).includes(to)) throw new Error(`illegal transition ${o.status} -> ${to}`);
      await tx`UPDATE orders SET status=${to} WHERE id=${orderId}`;
      if (to === 'paid') {
        const items = await tx`SELECT variant_id, quantity FROM order_items WHERE order_id=${orderId}`;
        for (const it of items) await tx`UPDATE inventory SET stock = stock - ${it.quantity} WHERE variant_id=${it.variant_id}`;
      }
    });
  }
  async getOrder(orderId: number) {
    const [o] = await this.sql`SELECT * FROM orders WHERE id=${orderId}`;
    if (!o) return null;
    o.items = await this.sql`SELECT oi.quantity, oi.unit_price_cents, v.title AS variant_title, p.title AS product_title FROM order_items oi JOIN variants v ON v.id=oi.variant_id JOIN products p ON p.id=v.product_id WHERE oi.order_id=${orderId}`;
    return o;
  }
  async stats() { const [p] = await this.sql`SELECT COUNT(*)::int n FROM products`; const [v] = await this.sql`SELECT COUNT(*)::int n FROM variants`; return { products: p.n, variants: v.n }; }
}
