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
