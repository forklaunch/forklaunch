import { queryTerms } from './ranking.service';

const MAX_TOPIC_LENGTH = 200;

/**
 * The search query for a follow-up question in a conversation about a
 * topic: "how is it treated?" after "heart attack" becomes "heart attack:
 * how is it treated?". A follow-up that already names the topic is used as
 * it is.
 */
export function followUpQuery(followUp: string, topic: string): string {
  const question = followUp.trim();
  const subject = topic.trim().slice(0, MAX_TOPIC_LENGTH);
  if (!subject) return question;
  const questionWords = new Set(queryTerms(question));
  const topicWords = queryTerms(subject);
  if (topicWords.length > 0 && topicWords.every((w) => questionWords.has(w))) {
    return question;
  }
  return `${subject}: ${question}`;
}

// Words that ask about one part of a topic rather than name it. Kept to a
// short list: a symptom word like "pain" names topics too ("chest pain").
const ASPECT_WORDS = new Set([
  'treatment', 'treatments', 'treat', 'treated', 'treating', 'therapy', 'therapies', 'management', 'manage',
  'managed', 'procedure', 'procedures', 'surgery', 'surgeries', 'operation', 'operations', 'cure',
  'symptoms', 'symptom', 'signs', 'diagnosis', 'diagnose', 'diagnosed', 'causes', 'cause', 'caused',
  'prognosis', 'outlook', 'complications', 'prevention', 'prevent', 'prevented',
  'side', 'effects', 'uses', 'used', 'indications', 'contraindications', 'interactions'
]);
// Joining words around an aspect: "procedure for", "treatment of",
// "what are the", "how is ... treated"
const CONNECTORS = new Set([
  'for', 'of', 'in', 'to', 'after', 'with', 'the', 'a', 'an', 'what', 'which', 'how', 'is', 'are', 'was',
  'can', 'do', 'does', 'be', 'it', 'its', 'and', 'or', 'best', 'main', 'common', 'usual'
]);

/**
 * Splits a question about one part of a topic into the topic and the
 * aspect: "procedure for heart attack" -> heart attack, asked about its
 * procedures; "cefazolin side effects" -> cefazolin. Undefined when the
 * query asks about no aspect ("heart attack") or names nothing else.
 */
export function splitAspectQuery(query: string): { topic: string; aspect: string } | undefined {
  const words = query.toLowerCase().replace(/[?!.,;:]+/g, ' ').split(/\s+/).filter(Boolean);
  const aspect = words.filter((w) => ASPECT_WORDS.has(w));
  if (aspect.length === 0) {
    return undefined;
  }
  const topic = words.filter((w) => !ASPECT_WORDS.has(w) && !CONNECTORS.has(w));
  if (topic.length === 0) {
    return undefined;
  }
  return { topic: topic.join(' '), aspect: aspect.join(' ') };
}
