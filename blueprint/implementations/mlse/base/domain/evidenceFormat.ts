import { EvidencePassageDto } from '@forklaunch/interfaces-mlse/types';

// Passage text comes from the internet. No tag inside it, in any case or
// with any name, may close the passage or the evidence block, so angle
// brackets are escaped in everything placed inside the markup.
export function escapeMarkup(text: string): string {
  return text.replaceAll('<', '&lt;').replaceAll('>', '&gt;');
}

export function formatEvidence(evidence: EvidencePassageDto[]): string {
  const passages = evidence.map((passage) => {
    const label = passage.label ? ` source="${escapeMarkup(passage.label).replaceAll('"', "'")}"` : '';
    return `<passage id="${escapeMarkup(passage.id)}"${label}>\n${escapeMarkup(passage.text)}\n</passage>`;
  });
  return `<evidence>\n${passages.join('\n')}\n</evidence>`;
}

// The user message every AI provider sends: the evidence, then the task.
export function userMessage(evidence: EvidencePassageDto[], prompt: string): string {
  return `${formatEvidence(evidence)}\n\n<task>\n${escapeMarkup(prompt)}\n</task>`;
}
