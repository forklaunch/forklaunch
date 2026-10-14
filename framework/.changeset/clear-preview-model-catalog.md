---
'@forklaunch/core': minor
---

Add `createPreviewModelGatewayClient` for the desktop's isolated local model relay. Its validated `GatewayPreviewModels` catalog contains model IDs and short-lived preview ceilings, without inventing managed monthly usage or BAA metadata. The existing managed `createModelGatewayClient` and `GatewayModels` contract are unchanged.

The additive `GatewayPreviewChatCompletion` type makes a provider timestamp optional; managed completion types retain their existing required `created` field.
