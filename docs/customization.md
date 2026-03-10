---
title: Customization
category: Guides
description: Customize documentation UI, authorization behavior, and validation in your ForkLaunch application.
---

## Overview

ForkLaunch provides several levels of customization for core features like documentation, authorization, and validation.

## Documentation

### Scalar UI Customization
When using the default Scalar documentation UI, you can customize:
```typescript
const docsConfiguration = {
  theme: {
    colors: {
      primary: '#4F46E5',
      secondary: '#7C3AED'
    },
    logo: '/path/to/logo.svg'
  },
  layout: {
    hideTryIt: false,
    hideExamples: false
  }
}

const app = forkLaunchApplication({
  ...,
  docsConfiguration
})
```

## Authorization

Authorization can be configured at the endpoint contract definition:

```typescript
router.get('/path', {
  ...,
  auth: {
    ...
  }
}, handler)
```

## Validation

Control validation behavior by endpoint:

```typescript
router.post('/path', {
  ...,
  validation: {
    request: 'error',
    response: 'warning'
  }
}, handler)
```

## Best Practices

1. Configure auth at the highest appropriate level
2. Use stricter validation where possible

## Default Values

If not specified, ForkLaunch uses these defaults:
- Documentation UI: Scalar
- Auth: none
- Validation: 'error' for both request and response