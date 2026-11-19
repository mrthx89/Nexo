# Nexo Windows Desktop (`nexo-desktop`) Design Direction

## Identity & Purpose
- **Product**: Nexo Desktop Appliance Client for Windows (Tauri v2 + Rust `nexo-core`)
- **Brand Persona**: Living silver robotic face (`NEXO Silver`) inside a calm, distraction-free dark cockpit built for continuous desktop/monitor standby.
- **Language**: Indonesian primary UI copy matching `nexo-android`, zero em-dashes (`—`), plain factual status copy.

## Three Antislop Dials
- **ENERGY**: `2` (Balanced depth: deep obsidian ground with one living mascot focal point)
- **RHYTHM**: `2` (Primary 3-zone voice stage; focused single-task dialogs for Credentials, Voiceprint/Diagnostic, and IoT Control)
- **MOTION**: `2` (Audio-reactive mascot eye glow & 4-bar mic waveform during active speech, smooth transitions between 17 expressions, calm dim in sleep mode; zero endless idle pulsing; honors `prefers-reduced-motion`)

## Color System (Verified WCAG AA Contrast)
- **Canvas Ground**: `#000000` (OLED pure black for monitor focus & wake-mode readability)
- **Primary Surface**: `#0C121B` (status capsule, reply card, dialogs)
- **Elevated Surface**: `#141D2B` (inputs, secondary buttons, device rows)
- **Subtle Hairline Border**: `#243247`
- **Primary Text**: `#E0E6ED` (16.71:1 on `#000000`, 14.95:1 on `#0C121B`)
- **Secondary Text**: `#8892B0` (6.07:1 on `#0C121B`)
- **Muted / Helper Text**: `#94A3B8` (7.33:1 on `#0C121B`)
- **Single Brand Focal Accent**: `#00E5FF` (Nexo Cyan, `#000000` text on `#00E5FF` = 13.65:1; used only on primary microphone action & key focus states)
- **Semantic State Indicators** *(functional state only, never decorative)*:
  - Ready / Listening: `#00E676` (11.25:1 on `#0C121B`)
  - Thinking / Reconnecting: `#FFB300` (10.47:1 on `#0C121B`)
  - Speaking: `#2979FF` (used on LED dot & waveform bar accent)
  - Error: `#FF5252` (5.89:1 on `#0C121B`)

## Typography
- **Font Stack**: `'Segoe UI Variable', 'Segoe UI', system-ui, -apple-system, sans-serif` (native Windows legibility, zero external font fetches)
- **Monospace** *(Device ID / SHA / Endpoint URLs only)*: `'Cascadia Code', 'Consolas', monospace`
- **Scale**:
  - Status & Compact Controls: `12px`–`13px` (`font-weight: 500–600`)
  - Transcript Reply & Dialog Body: `14px`–`15px` (`line-height: 1.55`)
  - Modal Headings: `18px` (`font-weight: 600`)

## Purpose-Gate Decisions & Dose Caps (R-31)
1. **Obsidian Dark Canvas (`#000000`)**: Reduces glare on an always-on desk monitor and frames the silver/cyan NEXO mascot eyes identically to `nexo-android`.
2. **Backdrop Blur Cap (`<= 2` surfaces)**: Applied only on `.status-capsule` and `.reply-card` so modals stay crisp, opaque `#0C121B`, and lightweight on WebView2 GPU RAM.
3. **Glow Accent Cap (`<= 2` elements)**: Applied only behind `.mascot-stage` (scaled by live microphone/TTS amplitude) and `.mic-button:focus-visible` / active ring.
4. **Border Radius Hierarchy**: Circular (`9999px`) reserved strictly for the primary microphone button and top status capsule; dialogs use `14px`, device cards use `10px`, inputs and action buttons use `8px`.
5. **Keyboard & Screen-State Discipline**: All triggers have accessible `aria-label` and focus rings (`outline: 2px solid #00E5FF`), `Escape` closes any open modal and restores focus, and every async panel renders explicit Loading, Empty (why empty + next action), and Error states.
