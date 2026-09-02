# Product Presentation accessibility loop

Product Presentation owns the meaning of its headings, labels, controls, links,
states, status messages, and alerts. Keep that meaning in ordinary Product
Workspace source; Yydra does not provide UI wrappers or a product-semantics DSL.

For every public presentation requirement:

1. Add or update the matching assertion in
   `frontend/e2e/product-presentation.accessibility.spec.ts` using Playwright
   role, accessible-name, label, and state locators.
2. Implement the semantic intent directly with the Supported Golden Stack
   Surface: React Native `accessibilityRole`, `accessibilityLabel`,
   `accessibilityState`, and visible child content.
3. Run `npm run test:product-semantics` for focused feedback, then finish with
   `yydra check .`.

Prefer assertions such as
`getByRole('heading', { name: title, exact: true })` over visual text or test-id
selectors when the requirement is semantic. Layout, styles, routes, and source
structure may change without changing these assertions.

The required H5 check proves only the Product-owned semantics registered in this
visible spec on the H5 Application Surface. It does not prove complete WCAG
conformance, Android/iOS assistive-technology behavior, or hidden Eval
acceptance.
