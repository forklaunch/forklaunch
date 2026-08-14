/**
 * Domain logic over the SQLite store: bulk import, catalog reads, cart,
 * checkout, and the order state machine. Mirrors the ForkLaunch ecommerce
 * module's intended behavior (ECOM-02/03/04/06/08/09/10/12).
 */
import { Database } from 'bun:sqlite';
import type { NormalizedCatalog } from './model.ts';

const DEFAULT_SEED_STOCK = 100;

/** Legal order transitions — the ECOM-07 state machine. */
const ORDER_TRANSITIONS: Record<string, string[]> = {
  pending: ['paid', 'cancelled'],
  paid: ['fulfilled', 'cancelled'],
  fulfilled: ['shipped', 'cancelled'],
  shipped: ['delivered'],
  delivered: [],
  cancelled: [],
};

export class Store {
  constructor(private db: Database) {}

  /** ECOM-03 bulk import: upsert products/variants, seed inventory. */
  importCatalog(cat: NormalizedCatalog): { products: number; variants: number } {
    const insProduct = this.db.query(`
      INSERT INTO products (external_id, handle, source_url, title, description_html, vendor, product_type, tags, options, image_src)
      VALUES ($ext, $handle, $url, $title, $desc, $vendor, $type, $tags, $options, $image)
      ON CONFLICT(external_id) DO UPDATE SET title=excluded.title, handle=excluded.handle
      RETURNING id`);
    const insVariant = this.db.query(`
      INSERT INTO variants (product_id, external_id, sku, title, option_values, price_cents, compare_at_price_cents, requires_shipping)
      VALUES ($pid, $ext, $sku, $title, $opts, $price, $cmp, $ship)
      ON CONFLICT(external_id) DO UPDATE SET price_cents=excluded.price_cents
      RETURNING id`);
    const insInv = this.db.query(`INSERT OR IGNORE INTO inventory (variant_id, stock) VALUES ($vid, $stock)`);

    let pCount = 0, vCount = 0;
    const tx = this.db.transaction(() => {
      for (const p of cat.products) {
        const row = insProduct.get({
          $ext: p.externalId, $handle: p.handle, $url: p.sourceUrl, $title: p.title,
          $desc: p.descriptionHtml, $vendor: p.vendor, $type: p.productType,
          $tags: JSON.stringify(p.tags), $options: JSON.stringify(p.options),
          $image: p.images[0]?.src ?? '',
        }) as { id: number };
        pCount++;
        for (const v of p.variants) {
          const vrow = insVariant.get({
            $pid: row.id, $ext: v.externalId, $sku: v.sku, $title: v.title,
            $opts: JSON.stringify(v.optionValues), $price: v.priceCents,
            $cmp: v.compareAtPriceCents, $ship: v.requiresShipping ? 1 : 0,
          }) as { id: number };
          insInv.run({ $vid: vrow.id, $stock: DEFAULT_SEED_STOCK });
          vCount++;
        }
      }
    });
    tx();
    return { products: pCount, variants: vCount };
  }

  listProducts(): any[] {
    return this.db.query(`SELECT id, handle, title, vendor, product_type, image_src FROM products ORDER BY title`).all();
  }

  getProductByHandle(handle: string): any | null {
    const p = this.db.query(`SELECT * FROM products WHERE handle = $h`).get({ $h: handle }) as any;
    if (!p) return null;
    p.tags = JSON.parse(p.tags || '[]');
    p.options = JSON.parse(p.options || '[]');
    p.variants = this.db.query(`
      SELECT v.*, i.stock FROM variants v JOIN inventory i ON i.variant_id = v.id
      WHERE v.product_id = $pid`).all({ $pid: p.id }).map((v: any) => {
      v.option_values = JSON.parse(v.option_values || '{}');
      return v;
    });
    return p;
  }

  seedCustomer(email: string, name: string, address: object): number {
    const row = this.db.query(`
      INSERT INTO customers (email, name, address) VALUES ($e, $n, $a)
      ON CONFLICT(email) DO UPDATE SET name=excluded.name RETURNING id`)
      .get({ $e: email, $n: name, $a: JSON.stringify(address) }) as { id: number };
    return row.id;
  }

  // --- Cart (ECOM-06) ---
  createCart(customerId?: number): number {
    const row = this.db.query(`INSERT INTO carts (customer_id) VALUES ($c) RETURNING id`)
      .get({ $c: customerId ?? null }) as { id: number };
    return row.id;
  }

  addToCart(cartId: number, variantId: number, qty: number) {
    this.db.query(`INSERT INTO cart_items (cart_id, variant_id, quantity) VALUES ($c,$v,$q)`)
      .run({ $c: cartId, $v: variantId, $q: qty });
  }

  getCart(cartId: number): any {
    const items = this.db.query(`
      SELECT ci.id, ci.quantity, v.id AS variant_id, v.title AS variant_title, v.price_cents,
             p.title AS product_title, p.handle, i.stock
      FROM cart_items ci
      JOIN variants v ON v.id = ci.variant_id
      JOIN products p ON p.id = v.product_id
      JOIN inventory i ON i.variant_id = v.id
      WHERE ci.cart_id = $c`).all({ $c: cartId }) as any[];
    const subtotal = items.reduce((n, it) => n + it.price_cents * it.quantity, 0);
    return { id: cartId, items, subtotalCents: subtotal };
  }

  // --- Checkout (ECOM-09) ---
  checkout(cartId: number, taxRate = 0.08): { orderId: number } {
    const cart = this.getCart(cartId);
    if (cart.items.length === 0) throw new Error('cart is empty');
    // stock validation (ECOM-04 in-stock check)
    for (const it of cart.items) {
      if (it.stock < it.quantity) throw new Error(`out of stock: ${it.product_title} / ${it.variant_title}`);
    }
    const cartRow = this.db.query(`SELECT customer_id FROM carts WHERE id=$c`).get({ $c: cartId }) as any;
    const subtotal = cart.subtotalCents;
    const tax = Math.round(subtotal * taxRate);
    const total = subtotal + tax;
    const tx = this.db.transaction(() => {
      const order = this.db.query(`
        INSERT INTO orders (customer_id, status, subtotal_cents, tax_cents, total_cents, created_at)
        VALUES ($cust, 'pending', $sub, $tax, $tot, $ts) RETURNING id`)
        .get({ $cust: cartRow?.customer_id ?? null, $sub: subtotal, $tax: tax, $tot: total, $ts: new Date().toISOString() }) as { id: number };
      for (const it of cart.items) {
        this.db.query(`INSERT INTO order_items (order_id, variant_id, quantity, unit_price_cents) VALUES ($o,$v,$q,$p)`)
          .run({ $o: order.id, $v: it.variant_id, $q: it.quantity, $p: it.price_cents });
      }
      this.db.query(`UPDATE carts SET status='converted' WHERE id=$c`).run({ $c: cartId });
      return order.id;
    });
    return { orderId: tx() };
  }

  // --- Order state machine (ECOM-08/10/12) ---
  transitionOrder(orderId: number, to: string): void {
    const order = this.db.query(`SELECT status FROM orders WHERE id=$o`).get({ $o: orderId }) as any;
    if (!order) throw new Error('order not found');
    const legal = ORDER_TRANSITIONS[order.status] ?? [];
    if (!legal.includes(to)) throw new Error(`illegal transition ${order.status} -> ${to}`);
    const tx = this.db.transaction(() => {
      this.db.query(`UPDATE orders SET status=$s WHERE id=$o`).run({ $s: to, $o: orderId });
      // ECOM-05: decrement inventory on paid
      if (to === 'paid') {
        const items = this.db.query(`SELECT variant_id, quantity FROM order_items WHERE order_id=$o`).all({ $o: orderId }) as any[];
        for (const it of items) {
          this.db.query(`UPDATE inventory SET stock = stock - $q WHERE variant_id=$v`).run({ $q: it.quantity, $v: it.variant_id });
        }
      }
    });
    tx();
  }

  /** Mock payment (ECOM-10 seam): always succeeds, drives pending -> paid. */
  pay(orderId: number): void {
    this.transitionOrder(orderId, 'paid');
  }

  getOrder(orderId: number): any {
    const o = this.db.query(`SELECT * FROM orders WHERE id=$o`).get({ $o: orderId }) as any;
    if (!o) return null;
    o.items = this.db.query(`
      SELECT oi.quantity, oi.unit_price_cents, v.title AS variant_title, p.title AS product_title
      FROM order_items oi JOIN variants v ON v.id=oi.variant_id JOIN products p ON p.id=v.product_id
      WHERE oi.order_id=$o`).all({ $o: orderId });
    return o;
  }

  stats(): { products: number; variants: number } {
    const p = (this.db.query(`SELECT COUNT(*) n FROM products`).get() as any).n;
    const v = (this.db.query(`SELECT COUNT(*) n FROM variants`).get() as any).n;
    return { products: p, variants: v };
  }
}
