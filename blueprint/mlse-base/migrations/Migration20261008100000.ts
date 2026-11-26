import { Migration } from '@mikro-orm/migrations';

// The literature searches each procedure page is built from: PubMed and PMC
// papers whose major subject is the procedure, so a page draws on papers
// about it rather than ones that mention it in passing; and, of those, the
// ones on surgical technique, since outcome studies rarely say how an
// operation is done.
const HEADINGS: Record<string, string> = {
  'laparoscopic-cholecystectomy': '"Cholecystectomy, Laparoscopic"[majr]',
  appendectomy: '"Appendectomy"[majr]',
  // Herniorrhaphy covers every hernia; the inguinal ones only
  'inguinal-hernia-repair': '"Herniorrhaphy"[majr] AND "Hernia, Inguinal"[majr]',
  'cesarean-section': '"Cesarean Section"[majr]',
  'total-knee-arthroplasty': '"Arthroplasty, Replacement, Knee"[majr]',
  'coronary-artery-bypass-grafting': '"Coronary Artery Bypass"[majr]'
};
const TECHNIQUE = '(technique[ti] OR techniques[ti] OR "step-by-step"[ti])';

const quote = (value: string) => `'${value.replace(/'/g, "''")}'`;

export class Migration20261008100000 extends Migration {

  override name = 'Migration20261008100000';

  override up(): void | Promise<void> {
    this.addSql(`alter table "topic" add column "corpus_queries" text[] not null default '{}';`);
    for (const [slug, heading] of Object.entries(HEADINGS)) {
      const queries = [heading, `${heading} AND ${TECHNIQUE}`];
      this.addSql(`update "topic" set "corpus_queries" = array[${queries.map(quote).join(', ')}] where "slug" = ${quote(slug)};`);
    }
  }

  override down(): void | Promise<void> {
    this.addSql(`alter table "topic" drop column "corpus_queries";`);
  }

}
