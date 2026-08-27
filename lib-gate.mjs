/**
 * Shared helpers for the storefront gates.
 *
 * Everything here exists to remove fixed sleeps. The first version of these
 * tests was built on `waitForTimeout(2500)` between every step, which made
 * them lie in both directions: they failed when the machine was loaded and a
 * render took 3 seconds, and they passed for the wrong reason when a step that
 * should have been instant quietly took two. One of those false failures —
 * "Stripe card frame never appeared" — cost an afternoon before the real cause
 * turned out to be a cart that had not finished saving.
 *
 * A test that waits for the condition it actually cares about is both faster
 * and honest about what it proved.
 */
import { execFileSync } from 'node:child_process';

/** Poll `fn` until it returns something truthy. Returns null on timeout. */
export async function until(fn, { timeout = 30000, every = 250, label = '' } = {}) {
  const deadline = Date.now() + timeout;
  let last = null;
  while (Date.now() < deadline) {
    try { last = await fn(); } catch { last = null; }
    if (last) return last;
    await new Promise((r) => setTimeout(r, every));
  }
  if (label) console.error(`  (timed out waiting for ${label})`);
  return null;
}

/** A results collector that survives a crash mid-run. */
export function recorder(title) {
  const results = [];
  return {
    check(name, pass, detail = '') { results.push({ name, pass: !!pass, detail }); },
    results,
    /**
     * Printed from a `finally`, so a thrown error still reports everything
     * proven up to that point. A crash that erases its own partial results
     * tells you only that something broke, not how far it got.
     */
    report(err) {
      const failed = results.filter((r) => !r.pass);
      console.log(`\n${title}\n`);
      for (const r of results) {
        console.log(`  ${r.pass ? 'PASS' : 'FAIL'}  ${r.name}${r.detail ? '   (' + r.detail + ')' : ''}`);
      }
      if (err) console.log(`\n  CRASHED after ${results.length} assertion(s): ${err.message}`);
      console.log(`\n${results.length - failed.length}/${results.length} passed\n`);
      return failed.length || err ? 1 : 0;
    }
  };
}

/** psql against a given database; the page's opinion never counts as evidence. */
export function sqlFor(url) {
  return (q) => execFileSync('psql', [url, '-tAc', q], { encoding: 'utf8' }).trim();
}

export const money = (c) => '$' + ((c || 0) / 100).toFixed(2);

/**
 * The Payment Element nests several iframes, and the one holding the card
 * fields is not at a stable index — Stripe reorganises its own markup. Find it
 * by the field it contains instead of by position.
 */
export async function cardFrame(page, timeout = 30000) {
  const f = await until(async () => {
    for (const fr of page.frames()) {
      if (!/stripe/i.test(fr.url())) continue;
      const has = await fr.evaluate(() => !!document.querySelector('input[name="number"]')).catch(() => false);
      if (has) return fr;
    }
    return null;
  }, { timeout, label: 'the Stripe card frame' });
  if (!f) throw new Error('Stripe card frame never appeared');
  return f;
}

export async function fillCard(page, frame, number) {
  await frame.fill('input[name="number"]', number);
  await frame.fill('input[name="expiry"]', '12 34');
  await frame.fill('input[name="cvc"]', '123');
  const zip = await frame.$('input[name="postalCode"]');
  if (zip) await zip.fill('94105');
}

/**
 * Submit the payment and wait for a definite outcome.
 *
 * `page.click('#pay')` alone is not reliable here. The Payment Element steals
 * focus while it finishes mounting, and the button often sits below the fold
 * behind a sticky summary, so Playwright's actionability check can resolve
 * against a spot the click never lands on. The symptom is silent: the test
 * proceeds, the webhook log shows payment_intent.created and never
 * payment_intent.succeeded, and every downstream assertion fails as though
 * payment were broken.
 *
 * Scrolling it into view and dispatching the click on the element itself
 * removes the ambiguity, and waiting for the page to actually respond means a
 * click that did nothing is reported as a click that did nothing rather than
 * as a payment failure.
 */
export async function pay(page) {
  await page.evaluate(() => document.getElementById('pay')?.scrollIntoView({ block: 'center' }));
  const fired = await page.evaluate(() => {
    const b = document.getElementById('pay');
    if (!b || b.disabled) return false;
    b.click();
    return true;
  });
  if (!fired) throw new Error('#pay was missing or disabled when the test tried to submit');

  // The page must acknowledge within a few seconds: a busy label, an error, or
  // the confirmation. Silence means the handler never ran.
  const acked = await until(() => page.evaluate(() => {
    const b = document.getElementById('pay');
    const err = (document.getElementById('err') || {}).textContent || '';
    const done = !!document.querySelector('.ok h1, .done h1');
    return done || err.trim().length > 0 || (b && /processing|…|\.\.\./i.test(b.textContent));
  }), { timeout: 15000, every: 300, label: 'the pay button to respond' });
  if (!acked) throw new Error('#pay was clicked but the page never responded');
  return true;
}
