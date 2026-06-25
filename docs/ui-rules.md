# Project Caesar: UI/UX Rules

The `caesar_console` dashboard is the sole interface between the tactical operator and the AI mesh network. It must project a premium, state-of-the-art aesthetic that inspires extreme confidence.

## 1. Premium Aesthetics ("The Wow Factor")
- **Dark Mode Native**: Bright white backgrounds are banned to preserve operator night vision.
- **Glassmorphism**: Use semi-transparent panels (`backdrop-filter: blur(10px)`) to create depth hierarchies over dynamic visual backgrounds or maps.
- **Vibrant Signifiers**: Use saturated, glowing colors for status indicators. Crimson (`#ef4444`) for threats, Emerald (`#10b981`) for safety. See [UI Tokens](ui-tokens.md) for the exact CSS properties.

## 2. Interaction & Micro-animations
An interface must feel responsive and alive.
- **Hover States**: Every interactive element must subtly transform on hover (e.g., `transform: translateY(-2px)`, expanded `box-shadow`).
- **Transitions**: Apply global transitions (`transition: all 0.3s ease-in-out`) to panels to avoid jarring DOM reflows.
- **Live Data Insertion**: When the Server-Sent Events (SSE) feed pushes a high-interest anomaly from the hub, the new card must slide or fade into the DOM smoothly. No instantaneous snapping.

## 3. Structural & Semantic Rigor
- **Vanilla Core**: TailwindCSS and React/Vue are not utilized. We rely on modern Vanilla CSS (Grid, Flexbox) and JS to ensure the console remains highly performant and easily hackable in the field without `npm` or build steps.
- **Explicit Identifiers**: All interactive elements must possess unique `id` attributes (e.g., `id="btn-reconfigure"`) so the Vanilla JS can reliably bind event listeners.
- **Semantic HTML**: UI templates must use proper semantic tags (`<header>`, `<main>`, `<article>`) rather than arbitrary `<div>` nesting.

## 4. Graceful Degradation
- **Stream Fallbacks**: If the edge node MJPEG stream (`/api/proxy-camera`) disconnects, the UI must intercept the `onerror` event and display a styled "LINK OFFLINE" placeholder, never a broken browser image icon.
- **Empty States**: If the mesh network detects zero threats, display a styled "No active threats detected" watermark.

*See [UI Registry](ui-registry.md) for the exact API endpoints and DOM IDs that map to these rules.*
