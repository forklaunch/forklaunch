---
name: design-system
description: "Design philosophy router: selects from 8 named design philosophies (Stripe, Linear, Robinhood, Fidelity, Clinical, Airbnb, Retool, Notion) and orchestrates design tools. Triggers on design/style/UI/frontend."
user-invokable: true
---

# Design Philosophy Router

## How This Works

Every app has a design philosophy, not just an industry. Robinhood and Fidelity are both "fintech" but look nothing alike. Stripe and AWS Console are both "dev tools" but from different planets.

**Step 1:** Identify the philosophy by asking:

> "Which of these feels closest to what you're building?
> 1. **Stripe** - clean developer craft, precision, API-docs-meet-dashboard
> 2. **Linear** - dark, fast, keyboard-driven, engineering-precision
> 3. **Robinhood** - consumer-simple, mobile-first, numbers-as-heroes
> 4. **Fidelity** - institutional authority, data-extreme, conservative
> 5. **Clinical** - healthcare trust, WCAG AAA, calming, accessible
> 6. **Airbnb** - marketplace warmth, photography-driven, discovery
> 7. **Retool** - ops density, max information, zero decoration
> 8. **Notion** - content canvas, typography-first, reading-optimized
>
> Or name a reference brand and I'll match it."

**Step 2:** Apply the matching philosophy below.
**Step 3:** Orchestrate tools (see Tool Stack at the bottom).

If the user names a brand not listed, map it:

| Brand | Philosophy |
|-------|-----------|
| Vercel, Supabase, Neon, Planetscale | Stripe |
| Raycast, Arc, Warp, Fig | Linear |
| Cash App, Venmo, Chime, Monzo | Robinhood |
| Bloomberg, Schwab, E*Trade, banks | Fidelity |
| One Medical, Headspace, MyChart, Calm | Clinical |
| DoorDash, Uber, Etsy, Instacart | Airbnb |
| Django admin, Airplane, internal tools | Retool |
| Substack, Medium, Ghost, Coda | Notion |
| Legora, DocuSign, legal platforms | Fidelity (conservative variant) |

---

## Universal Bans (All Philosophies)

NEVER use, regardless of philosophy:
- Side-stripe borders (`border-left: 3px solid`) on cards or list items
- Gradient text (`background-clip: text` with gradient)
- Glassmorphism as decoration (blur, glass cards, glow borders)
- Nested cards (cards inside cards)
- Identical repeated card grids (icon + heading + text x12)
- Pure black (#000) or pure white (#fff). Always tint.
- Bounce or elastic easing
- Purple-to-blue gradients, cyan-on-dark, neon accents (the "AI palette")
- Monospace as lazy "technical" shorthand
- Overused fonts: Inter, DM Sans, Space Grotesk, Plus Jakarta Sans, Instrument Sans, Outfit, Syne

---

## Philosophy 1: Stripe (Developer Craft)

**References:** Stripe, Vercel, Supabase, Neon, Planetscale
**Emotional register:** Craft, precision, quiet confidence
**User:** Developers and technical product managers

### Typography
- **Display:** Whyte, GT America, or Untitled Sans (geometric with subtle personality)
- **Body:** Same family, lighter weight. One family throughout.
- **Monospace:** Jetbrains Mono or Berkeley Mono (for code, API keys, IDs)
- **Scale:** 1.2 ratio. Clean, not cramped.

### Color
- **Theme:** Light mode default, dark mode polished. Both must be first-class.
- **Surface:** Warm gray, barely tinted toward brand hue (oklch 97% L, 0.005 C)
- **Accent:** Indigo or violet. One accent color used sparingly.
- **Borders:** Extremely subtle (oklch 90% L). Define space without shouting.
- **Code blocks:** Slightly tinted background, not pure gray.

### Layout
- **Top nav + sidebar hybrid.** Clean horizontal nav, contextual sidebar for deep pages.
- **Card-based metric displays.** Number, trend indicator, sparkline. No labels longer than 2 words.
- **API docs integration.** Code samples live next to prose. Three-column layout for API reference.
- **Generous padding.** Breathes more than Linear, less than Airbnb.
- **Rounded corners:** 8-12px. Soft but not bubbly.

### Components
- Metric cards: number, delta arrow, sparkline
- Tabbed interfaces for related views
- Inline code with subtle background tint
- Toast notifications (never modals for confirmations)
- Themable components (user can match their brand)

### Motion
- 150ms transitions. Snappy, not flashy.
- No page transitions. Instant routing.
- Subtle hover states (background tint, not border changes)

---

## Philosophy 2: Linear (Precision Engineering)

**References:** Linear, Raycast, Arc, Warp
**Emotional register:** Speed, precision, opinionated
**User:** Engineering teams, power users

### Typography
- **Display:** Inter Tight or Geist (exception to the ban list: Linear earned it)
- **Body:** Same. One font. Multiple weights.
- **Monospace:** SF Mono or JetBrains Mono
- **Scale:** 1.15 ratio. Tight, dense, information-rich.

### Color
- **Theme:** Dark mode default. Light mode as option. Both built with LCH/OKLCH for perceptual uniformity.
- **Surface:** Near-black with cool blue-gray tint (oklch 13% L, 0.01 C, hue 260)
- **Accent:** Purple or blue-violet. Gradient sphere for logo/hero moments ONLY (never on text or UI elements).
- **Borders:** oklch 20-25% L. Subtle grid lines.
- **Status colors:** Muted, never saturated. Status is conveyed by label + icon, not color alone.

### Layout
- **Sidebar (collapsible) + main content.** Sidebar is icon-only when collapsed.
- **Keyboard-first navigation.** Every action has a shortcut. Cmd+K command palette required.
- **List views over card views.** Dense, scannable, sortable.
- **Split view for detail.** Click an item, detail panel slides in.
- **No hero sections.** Content starts immediately.

### Components
- Command palette (Cmd+K) with fuzzy search
- List items with inline status, assignee avatar, priority icon
- Keyboard shortcut hints in tooltips and menus
- Context menus (right-click) on every actionable item
- Compact date pickers, inline editing
- Breadcrumbs with scope indicators

### Motion
- 100ms transitions. Everything feels instant.
- View transitions: crossfade, 120ms
- List reorder: spring physics, subtle
- No decorative animation. Motion serves function only.

---

## Philosophy 3: Robinhood (Consumer Simplicity)

**References:** Robinhood, Cash App, Venmo, Chime, Monzo
**Emotional register:** Friendly, inviting, never intimidating
**User:** General consumers, first-time users of complex domains

### Typography
- **Display:** Rethink Sans, General Sans, or Outfit (warm geometric, rounded terminals)
- **Body:** Same family. 16-18px minimum.
- **Numbers:** Large, bold, hero treatment. The number IS the interface.
- **Scale:** 1.333 ratio (perfect fourth). High contrast between heading and body.

### Color
- **Theme:** Light mode default. Dark mode for evening/night use.
- **Surface:** Clean white or warm cream
- **Primary:** Brand-saturated. Green for gains, red for losses (these are semantic, not decorative).
- **Color-as-data:** Color communicates state, not decoration. Green = positive, red = negative, gray = neutral.
- **Backgrounds:** Subtle pastels for card differentiation, never borders.

### Layout
- **Mobile-first, always.** Design for phone. Desktop is a stretched phone.
- **One number per screen.** The hero metric dominates. Everything else is secondary.
- **Bottom navigation bar** on mobile (4-5 items max).
- **Full-bleed cards.** Edge-to-edge on mobile, contained on desktop.
- **Scroll-driven.** Vertical scroll for discovery, horizontal for categories.

### Components
- Portfolio value: one giant number, change indicator, chart
- Transaction lists: merchant, amount, timestamp. Minimal.
- Onboarding flows: one question per screen, progress indicator
- Bottom sheets for actions (not modals)
- Confetti/celebration for milestones (used sparingly, once per achievement)

### Motion
- 250-300ms transitions. Smooth, satisfying, not snappy.
- Pull-to-refresh on mobile
- Number counting animation for portfolio value
- Page transitions: horizontal slide for navigation depth
- Haptic-aware (design for tactile feedback even if not implemented)

---

## Philosophy 4: Fidelity (Institutional Authority)

**References:** Fidelity, Bloomberg (lite), Schwab, E*Trade, Legora, traditional banks, insurance portals
**Emotional register:** Trust, authority, conservatism
**User:** Professional investors, institutional users, compliance officers, legal teams

### Typography
- **Display:** Freight Text, Tiempos, or Sentinel (serif signals authority)
- **Body:** Atkinson Hyperlegible or Source Sans 3 (legibility-first)
- **Numbers:** Tabular figures everywhere (`font-variant-numeric: tabular-nums`). Right-aligned.
- **Monospace:** For account numbers, policy numbers, transaction IDs
- **Scale:** 1.125 ratio. Maximum density. 13-14px body text.

### Color
- **Theme:** Light mode default. Dark mode for trading terminals only.
- **Surface:** Cool off-white (oklch 96% L, 0.005 C, hue 220)
- **Primary:** Dark navy blue (oklch 30% L). The default "serious money" color.
- **Accent:** Forest green for positive, deep red for negative. Both muted.
- **Chrome:** Heavy use of borders, dividers, and background bands to create structure.
- **Security cues:** Padlock icons, "SSL Secured" badges, trust marks.

### Layout
- **Data tables dominate.** Full-width, dense, sortable, filterable, exportable.
- **Multi-panel dashboards.** 2-3 column layouts with independent scroll.
- **Tabs for account switching.** Every section tabbed.
- **Print-friendly layouts.** Statements, reports, and summaries must print cleanly.
- **No infinite scroll.** Always paginated. Users need to feel "I've seen everything."

### Components
- Account summary cards with balance, account number, last activity
- Holdings tables with position, shares, cost basis, market value, gain/loss
- Document center: statements, tax forms, confirmations (PDF links)
- Secure messaging with encryption indicators
- Multi-step forms with progress bars and save-draft capability
- Disclaimers and fine print (legally required, styled as footnotes)

### Motion
- Near zero. 100ms for essential state changes only.
- No decorative animation.
- Loading spinners for data fetches. Never skeleton loading (skeletons feel unfinished in institutional contexts).
- Page transitions: instant. No slide, no fade.

---

## Philosophy 5: Clinical (Healthcare Trust)

**References:** One Medical, Headspace, MyChart (Epic), Calm, Zocdoc
**Emotional register:** Trust, calm, warmth, accessibility
**User:** Patients, clinicians, caregivers. Often stressed, sometimes elderly.

### Typography
- **Display:** Libre Baskerville or Source Serif 4 (warmth without fussiness)
- **Body:** Source Sans 3, Nunito, or Lato. Round terminals = approachable.
- **Scale:** 1.25 ratio. Generous sizing. 16-18px body minimum.
- **Weight:** 400 minimum for body. Never thin weights. Readability > aesthetics.

### Color
- **Theme:** Light mode only (default). Dark mode only if clinician-facing night-shift tool.
- **Surface:** Warm off-white (oklch 97% L, 0.01 C, hue 80-90)
- **Primary:** Teal or muted blue. NEVER red as primary (red = emergency).
- **Secondary:** Sage green, warm gray, soft lavender
- **Warning:** Amber for non-critical. Reserve red for true clinical emergencies.
- **Contrast:** WCAG AAA (7:1). Non-negotiable for all text.

### Layout
- **Single-column primary content.** Side panels for supplementary info only.
- **Large touch targets.** 44x44px minimum. No exceptions.
- **Generous whitespace.** These users are stressed. Don't add cognitive load.
- **No infinite scroll.** Patients need to see "I've read everything."
- **Clear section dividers.** Generous spacing or subtle horizontal rules.

### Components
- Appointment cards: date, time, provider, type. Large, tappable.
- Medication lists: drug, dose, frequency, refill status, interactions
- Lab results with normal-range indicators (not just numbers)
- Consent forms with explicit checkboxes (never assumed consent)
- Secure messaging with read receipts
- Loading states: "Loading your records securely" (never just a spinner)

### Motion
- Minimal. `prefers-reduced-motion` respected always.
- 200ms for state changes.
- No entrance animations. Content present immediately.
- Progress indicators for any action > 1 second.

### Accessibility (Non-Negotiable)
- WCAG AAA
- Full keyboard navigation
- Screen reader tested
- Text resizable to 200% without breaking
- High contrast mode supported
- Color never as sole information carrier

---

## Philosophy 6: Airbnb (Marketplace Warmth)

**References:** Airbnb, DoorDash, Uber, Etsy, Instacart
**Emotional register:** Warmth, discovery, delight
**User:** Consumers browsing, selecting, purchasing

### Typography
- **Display:** Clash Display, Rethink Sans Bold (bold, friendly, slightly playful)
- **Body:** Nunito Sans, Lato (warm, approachable)
- **Scale:** 1.333 ratio. Large headlines, clear body.

### Color
- **Theme:** Light default. Dark for evening UX (delivery apps).
- **Surface:** White or warm cream
- **Primary:** Brand color, saturated. Bold. Impossible to miss.
- **Category chips:** Pastel backgrounds with dark text.

### Layout
- **Mobile-first.** Phone design first, desktop adapts.
- **Search prominent.** Top of every browse page.
- **Horizontal carousels** for categories.
- **Card-based discovery.** Image, name, rating, price, key detail.
- **Sticky bottom bar** for cart/checkout on mobile.
- **Filters as horizontal chips**, not sidebar checkboxes.

### Components
- Product/listing cards: large image, name, rating (stars), price, key attribute
- Cart drawer (slide from right, not full page)
- Map integration for location-based products
- Review displays with rating distribution
- Photo galleries with pinch-to-zoom
- Price breakdowns in checkout (subtotal, fees, tax, total)

### Motion
- 250-300ms. Smooth, satisfying.
- Pull-to-refresh on mobile.
- Card press: subtle scale-down (0.98) on press, bounce on release.
- Page transitions: slide for depth, fade for same-level.
- Skeleton loading for images and cards.

---

## Philosophy 7: Retool (Ops Density)

**References:** Retool, Django admin, Airplane, internal ops tools
**Emotional register:** Functional, efficient, no-nonsense
**User:** Internal teams, 8 hours/day in the tool

### Typography
- **Display:** System font stack (`-apple-system, BlinkMacSystemFont, "Segoe UI", Roboto`). No custom fonts.
- **Body:** Same. 13-14px base.
- **Monospace:** For IDs, timestamps, JSON, logs.
- **Scale:** 1.125 ratio. Maximum density.

### Color
- **Theme:** Light default (easier for long reading). Dark as option.
- **Surface:** Functional. Light gray background, white content areas.
- **Accent:** Blue for links and primary actions. Nothing else.
- **Status:** Traffic-light (green/amber/red) with text labels. Never color-only.
- **No brand colors.** This is a tool, not marketing.

### Layout
- **Full-width tables.** No max-width. Use all screen space.
- **Collapsible sidebar.** Icon-only when collapsed.
- **Split-pane for detail.** Master-detail pattern.
- **Tabs for related views.**
- **No hero sections. No decorative elements.**

### Components
- Data tables: sortable, filterable, bulk-selectable, inline-editable, paginated
- Global search across all resources
- Audit log viewer
- JSON viewer for raw inspection
- Quick filter toggles above tables
- Keyboard shortcuts documented in ? modal

### Motion
- None. Zero. Instant state changes.
- Loading spinners for async only.
- Skeleton loading for tables only.

---

## Philosophy 8: Notion (Content Canvas)

**References:** Notion, Substack, Medium, Ghost, Coda
**Emotional register:** Thoughtful, spacious, reading-optimized
**User:** Writers, knowledge workers, content creators

### Typography
- **Display:** Newsreader, Literata, or Lora (literary serif)
- **Body:** Same serif for long-form. 18-20px. Line height 1.6-1.7.
- **Monospace:** For inline code and technical content.
- **Scale:** 1.25 ratio. Generous.
- **Line length:** 60-70 characters max. Non-negotiable.

### Color
- **Theme:** Light default (reading). Dark mode well-crafted.
- **Surface:** Warm white (oklch 98% L, 0.008 C, hue 50-60)
- **Accent:** Single muted tone. Used for links and interactive elements only.
- **Text:** Near-black, never pure black (oklch 15% L, tinted toward warm)
- **Minimal color overall.** Typography and spacing do the work.

### Layout
- **Single column, centered.** 720px max width for prose.
- **Block-based content.** Each paragraph, heading, image, callout is a block.
- **Generous vertical spacing.** 24-32px between blocks.
- **Sticky table of contents** on the side for long documents.
- **Full-bleed images** that break the content column width.

### Components
- Block editor (paragraph, heading, list, callout, code, image, divider)
- Table of contents (auto-generated from headings)
- Inline mentions and links
- Callout boxes (tip, warning, info) with left-accent color
- Toggle/accordion for collapsible sections
- Comments and annotations in the margin

### Motion
- Minimal. 200ms for UI state changes.
- Smooth scroll for anchor navigation.
- Block insertion animation: fade + slide-down, 250ms.
- No page transitions.

---

## Mixing Philosophies

Users can combine: "Stripe craft with Robinhood's mobile-first approach" is valid. When mixing:

1. Pick the PRIMARY philosophy for overall structure and typography.
2. Pick the SECONDARY for specific component patterns or interaction style.
3. Never mix more than two. Three philosophies create incoherence.
4. When in conflict, the primary wins on layout and typography, secondary wins on motion and interaction patterns.

---

## Tool Orchestration

Install these design tools once:

```bash
npx skills add anthropics/frontend-design    # Base: anti-generic aesthetics
npx skills add pbakaus/impeccable            # Execution: teach/craft/extract
npx skills add shadcn-ui/ui                  # Components: React + Tailwind
npx skills add nextlevelbuilder/ui-ux-pro-max-skill  # Reference: 161 palettes, 57 font pairings
npx skills add chrisvoncsefalvay/claude-d3js-skill   # Data viz: D3.js
npx skills add hamen/material-3-skill        # Android/Flutter: Material 3
```

### Workflow

1. **Context:** Run `/impeccable teach` with the selected philosophy's traits as design context.
2. **Components:** `npx shadcn@latest init` if React. Add components as needed.
3. **Palette:** Reference UI/UX Pro Max for font pairing inspiration. Apply philosophy-specific rules.
4. **Build:** Use `/impeccable craft` for the shape-then-build workflow.
5. **Audit:** Check against the philosophy's rules. Run `/impeccable extract` for design tokens.

### The Test

After building, ask: "If someone saw this and I said 'an AI made it,' would they believe it instantly?" If yes, you used defaults. Go back and make intentional choices. A designed interface makes people ask "how was this made?" not "which AI made this?"

Sources: [impeccable.style](https://impeccable.style/), [Stripe Dashboard](https://docs.stripe.com/stripe-apps/design), [Linear Method](https://linear.app/method/introduction), [Robinhood Design](https://robinhood.com/us/en/newsroom/the-top-secret-robinhood-design-story/)
