TouchGrass Brand Guidelines
===========================

Brand Essence
-------------

- **Voice**: Playful, gently sarcastic, never mean.
- **Promise**: Tiny, offline nudges that help you step away.
- **Taglines**: Rotate between "Go touch grass.", "Micro-breaks, macro-you.", "Be less chair.", "You sure you are 20s?"
- **Product Name**: TouchGrass (CamelCase). Short form TG is acceptable for icons, menu titles, and constrained spaces.

Color Palette
-------------

| Role                | Hex      | RGB          | CMYK                 | Usage                                                         |
| ------------------- | -------- | ------------ | -------------------- | ------------------------------------------------------------- |
| Grass Green         | #2D8659  | 45, 134, 89  | 66, 0, 34, 47        | Primary buttons, key highlights, brand mark                   |
| Deep Navy           | #1A3A52  | 26, 58, 82   | 68, 29, 0, 68        | Primary text, navigation, icons                               |
| Soft White          | #FAFAF9  | 250, 250, 249| 0, 0, 0, 2           | Light theme backgrounds                                       |
| Sage Green          | #A8D5BA  | 168, 213, 186| 21, 0, 13, 16        | Hover states, subtle fills, secondary highlights              |
| Charcoal Gray       | #2C3E50  | 44, 62, 80   | 45, 22, 0, 69        | Body text, borders, secondary icons                           |
| Light Gray          | #E8ECEF  | 232, 236, 239| 3, 1, 0, 6           | Disabled controls, dividers, inactive states                  |

Dark Mode Palette (default)
---------------------------

- **Background**: #121212
- **Surface panels**: #1E1E1E
- **Primary text**: #FFFFFF
- **Secondary text**: #C5CED8
- **Interactive highlights**: Grass Green (#2D8659)
- **Dividers**: rgba(255, 255, 255, 0.08)
- Always ship dark mode as the default experience. Light mode is opt-in and mirrors the brand palette above.

Typography
----------

- **Primary typeface**: Inter (fallback: system UI sans)
- **Weights**: 400 for body, 600 for headings, 500 for buttons
- **Character**: Slightly tighter letter-spacing (-0.5%) for headings to feel modern

Iconography & Illustration
--------------------------

- Flat, minimal icons with rounded corners and gentle motion
- Use Grass Green or Deep Navy for solid icons; outline icons should be 1.5 px strokes
- Illustrations should feel playful and encouraging (e.g., stylized plants, stretching figures). No guilt-tripping visuals.

Tone & Copy
-----------

- Friendly sarcasm is welcome ("Go touch grass.") but never shaming.
- Keep sentences short, conversational, and human.
- Highlight the benefits of taking breaks rather than the consequences of not doing so.

UI Guidelines
-------------

- Default to dark mode. Offer a light-theme toggle inside settings.
- Buttons: 12 px corner radius, Grass Green background, Soft White text.
- Disabled state: Light Gray background with 50% opacity text.
- Panels and modals: Surface color with 16 px padding and 8 px rounded corners.
- Notifications: Title in Deep Navy (light theme) or Soft White (dark theme) with optional emoji accent.

Sound Design
------------

- Default tone: soft, organic (think chime or wind note). Length under 1.5 seconds.
- Provide volume slider and mute toggle. Do not loop or repeat automatically.

Usage Guardrails
----------------

- Apply the brand palette consistently across desktop app, website, and collateral.
- Never stretch or skew the wordmark. Maintain clear space equal to the height of the letter T on all sides.
- Avoid using more than two accent colors in a single view to keep the UI calm.
- Ensure low-contrast backgrounds still pass accessibility ratios with text overlays.

Asset Roadmap
-------------

- Wordmark and icon set to be produced once UI scaffolding is live.
- Build vector assets (SVG) for tray icon in light and dark variants (TG initials with Grass Green accent).
- Prepare notification illustrations sized for 320x160 px canvases.

Questions about the brand? Ping the design channel before shipping new visuals.
