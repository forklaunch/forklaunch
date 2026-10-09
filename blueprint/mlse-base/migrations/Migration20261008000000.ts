import { Migration } from '@mikro-orm/migrations';

// A topic's own search words, by framework item, added to the framework's
// words that fit any procedure. Draft, like the frameworks: written by
// engineering, to be reviewed by a clinician with each page.
const HINTS: Record<string, Record<string, string[]>> = {
  'laparoscopic-cholecystectomy': {
    why: ['cholelithiasis', 'cholecystitis'],
    how: ['trocar', 'clipped', 'pneumoperitoneum'],
    access: ['trocar', 'pneumoperitoneum', 'insufflation', 'umbilical'],
    core: ['critical', 'cystic', 'clip', 'triangle']
  },
  appendectomy: {
    why: ['appendicitis', 'perforated', 'abscess'],
    when: ['nonoperative', 'antibiotics', 'interval'],
    how: ['laparoscopic', 'open', 'stump'],
    access: ['umbilical', 'mcburney', 'pneumoperitoneum'],
    core: ['mesoappendix', 'stump', 'base'],
    risks: ['abscess', 'stump', 'wound'],
    recovery: ['discharge', 'stay']
  },
  'inguinal-hernia-repair': {
    why: ['groin', 'incarcerated', 'strangulated'],
    when: ['watchful', 'waiting', 'asymptomatic'],
    how: ['mesh', 'lichtenstein', 'tep', 'tapp', 'laparoscopic'],
    access: ['inguinal', 'preperitoneal'],
    core: ['mesh', 'sac', 'cord', 'fixation'],
    risks: ['recurrence', 'chronic', 'seroma'],
    outcomes: ['recurrence']
  },
  'cesarean-section': {
    why: ['labor', 'labour', 'breech', 'fetal', 'previa'],
    who: ['vbac', 'previous'],
    when: ['planned', 'emergency', 'gestation', 'weeks'],
    how: ['pfannenstiel', 'transverse', 'uterine'],
    anesthesia: ['spinal', 'epidural', 'neuraxial'],
    positioning: ['tilt', 'lateral', 'displacement'],
    access: ['pfannenstiel', 'transverse', 'uterotomy'],
    core: ['uterotomy', 'delivery', 'placenta'],
    blood: ['oxytocin', 'postpartum', 'uterotonic'],
    closure: ['uterine', 'layer', 'skin'],
    risks: ['hemorrhage', 'endometritis', 'accreta'],
    after: ['breastfeeding', 'ambulation', 'thromboprophylaxis'],
    outcomes: ['neonatal', 'maternal']
  },
  'total-knee-arthroplasty': {
    why: ['osteoarthritis', 'arthritis', 'deformity'],
    who: ['obesity', 'bmi'],
    how: ['cemented', 'alignment', 'component', 'tourniquet'],
    anesthesia: ['spinal', 'nerve', 'block', 'adductor'],
    positioning: ['tourniquet', 'leg'],
    access: ['parapatellar', 'arthrotomy', 'medial'],
    core: ['tibial', 'femoral', 'resection', 'alignment', 'implant'],
    blood: ['tranexamic', 'tourniquet'],
    closure: ['capsule', 'drain'],
    risks: ['infection', 'stiffness', 'thromboembolism', 'loosening', 'revision'],
    after: ['physiotherapy', 'rehabilitation', 'mobilization'],
    outcomes: ['survivorship', 'revision', 'function', 'satisfaction']
  },
  'coronary-artery-bypass-grafting': {
    why: ['multivessel', 'stenosis', 'angina'],
    who: ['diabetes', 'ejection', 'syntax'],
    how: ['graft', 'anastomosis', 'cardiopulmonary', 'pump'],
    anesthesia: ['heparin', 'transesophageal'],
    access: ['sternotomy', 'median'],
    core: ['anastomosis', 'graft', 'mammary', 'saphenous', 'cardioplegia'],
    blood: ['protamine', 'heparin'],
    closure: ['sternal', 'wires'],
    emergence: ['ventilation', 'icu'],
    risks: ['stroke', 'fibrillation', 'mediastinitis'],
    outcomes: ['patency', 'survival']
  }
};

const quote = (value: string) => `'${value.replace(/'/g, "''")}'`;

export class Migration20261008000000 extends Migration {

  override name = 'Migration20261008000000';

  override up(): void | Promise<void> {
    this.addSql(`alter table "topic" add column "search_hints" jsonb null;`);

    // More procedure pages, drafts like the first. Each name is distinctive
    // in full: a topic page counts a document only if it names the topic.
    // Inguinal hernia repair has no MeSH descriptor of its own
    // (Herniorrhaphy covers every hernia), so it matches by name only.
    const topics: [slug: string, title: string, mesh: string | null, terms: string[]][] = [
      ['appendectomy', 'Appendectomy', 'D001062', ['appendectomy', 'appendicectomy']],
      ['inguinal-hernia-repair', 'Inguinal hernia repair', null, ['inguinal hernia', 'inguinal herniorrhaphy', 'inguinal hernioplasty']],
      ['cesarean-section', 'Cesarean section', 'D002585', ['cesarean section', 'caesarean section', 'cesarean delivery', 'caesarean delivery']],
      ['total-knee-arthroplasty', 'Total knee arthroplasty', 'D019645', ['knee arthroplasty', 'knee replacement']],
      ['coronary-artery-bypass-grafting', 'Coronary artery bypass grafting', 'D001026', ['coronary artery bypass', 'cabg']]
    ];
    for (const [slug, title, mesh, terms] of topics) {
      this.addSql(`insert into "topic" ("id", "created_at", "updated_at", "slug", "topic_type", "title", "framework_key", "mesh_descriptor_ui", "search_terms", "status") values (gen_random_uuid(), now(), now(), ${quote(slug)}, 'procedure', ${quote(title)}, 'procedure-v1', ${mesh ? quote(mesh) : 'null'}, array[${terms.map(quote).join(', ')}], 'draft') on conflict ("slug") do nothing;`);
    }

    for (const [slug, hints] of Object.entries(HINTS)) {
      this.addSql(`update "topic" set "search_hints" = ${quote(JSON.stringify(hints))}::jsonb where "slug" = ${quote(slug)};`);
    }
  }

  override down(): void | Promise<void> {
    this.addSql(`delete from "topic" where "slug" in ('appendectomy', 'inguinal-hernia-repair', 'cesarean-section', 'total-knee-arthroplasty', 'coronary-artery-bypass-grafting');`);
    this.addSql(`alter table "topic" drop column "search_hints";`);
  }

}
