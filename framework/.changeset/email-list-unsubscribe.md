---
'@forklaunch/core': minor
---

`createEmailClient().send` takes an optional `listUnsubscribe: { url?, mailto?, oneClick? }`. The platform adds `List-Unsubscribe` (RFC 2369) and, with `oneClick`, `List-Unsubscribe-Post: List-Unsubscribe=One-Click` (RFC 8058) to the message, which Gmail and Yahoo require of bulk senders. These are targets, not header text: `url` must be https (printable ASCII, up to 512 characters, no `< > , " \`), `mailto` a plain mailbox, at least one is required, and `oneClick` needs `url` (also enforced by the `EmailListUnsubscribe` type). The client, the platform gateway and the local gateway mock apply the same rules; the mock records the option and the headers it implies on `GET /__mock/email/messages`. `validateListUnsubscribe` and `EMAIL_LIMITS.maxUnsubscribeUrlLength` are exported.

Needs a platform whose email gateway accepts `listUnsubscribe`; an older gateway ignores the field.
