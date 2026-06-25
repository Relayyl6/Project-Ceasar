# Project Caesar: UI Tokens & CSS Variables

The following variables dictate the exact visual language of the Caesar Console. They must be placed in `index.css` (or equivalent) and used universally across the UI to ensure strict compliance with the premium glassmorphic aesthetic.

## CSS Custom Properties
```css
:root {
  /* ========================================= */
  /* COLOR PALETTE                             */
  /* ========================================= */
  
  /* Backgrounds: Deep Slate & Midnight */
  --bg-primary: #0f172a;       /* Outer edge of background gradients */
  --bg-secondary: #1e293b;     /* Core background or solid app color */
  --bg-panel: rgba(30, 41, 59, 0.7); /* Standard glassmorphic panel */
  --bg-panel-hover: rgba(30, 41, 59, 0.9);
  
  /* Accents: Vivid Sky Blue */
  --accent-primary: #38bdf8;   
  --accent-glow: rgba(56, 189, 248, 0.4);
  --accent-hover: #7dd3fc;
  
  /* Text */
  --text-primary: #f8fafc;     /* Main headers and values */
  --text-secondary: #94a3b8;   /* Labels and subtext */
  --text-tertiary: #64748b;    /* Disabled or placeholder text */
  
  /* Status / Threat Levels */
  --status-ok: #10b981;        /* Emerald - "monitor" / normal */
  --status-ok-bg: rgba(16, 185, 129, 0.15);
  
  --status-warn: #f59e0b;      /* Amber - warnings */
  --status-warn-bg: rgba(245, 158, 11, 0.15);
  
  --status-danger: #ef4444;    /* Crimson - "high-interest" / anomalies */
  --status-danger-bg: rgba(239, 68, 68, 0.15);
  --status-danger-glow: rgba(239, 68, 68, 0.4);

  /* ========================================= */
  /* TYPOGRAPHY                                */
  /* ========================================= */
  
  --font-sans: 'Inter', -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Helvetica, Arial, sans-serif;
  --font-mono: 'JetBrains Mono', 'Fira Code', Consolas, monospace;
  
  /* Font Sizes */
  --text-xs: 0.75rem;   /* 12px */
  --text-sm: 0.875rem;  /* 14px */
  --text-base: 1rem;    /* 16px */
  --text-lg: 1.125rem;  /* 18px */
  --text-xl: 1.25rem;   /* 20px */
  --text-2xl: 1.5rem;   /* 24px */
  --text-3xl: 1.875rem; /* 30px */

  /* ========================================= */
  /* SPACING & LAYOUT                          */
  /* ========================================= */
  
  --space-1: 0.25rem;  /* 4px */
  --space-2: 0.5rem;   /* 8px */
  --space-3: 0.75rem;  /* 12px */
  --space-4: 1rem;     /* 16px */
  --space-6: 1.5rem;   /* 24px */
  --space-8: 2rem;     /* 32px */
  --space-12: 3rem;    /* 48px */

  /* ========================================= */
  /* BORDERS & RADIUS                          */
  /* ========================================= */
  
  --radius-sm: 4px;
  --radius-md: 8px;
  --radius-lg: 12px;
  --radius-xl: 16px;
  --radius-full: 9999px;
  
  --border-glass: 1px solid rgba(255, 255, 255, 0.1);
  --border-glass-strong: 1px solid rgba(255, 255, 255, 0.2);

  /* ========================================= */
  /* EFFECTS (SHADOWS & TRANSITIONS)           */
  /* ========================================= */
  
  --shadow-sm: 0 1px 2px 0 rgba(0, 0, 0, 0.05);
  --shadow-md: 0 4px 6px -1px rgba(0, 0, 0, 0.1), 0 2px 4px -1px rgba(0, 0, 0, 0.06);
  --shadow-lg: 0 10px 15px -3px rgba(0, 0, 0, 0.1), 0 4px 6px -2px rgba(0, 0, 0, 0.05);
  
  --transition-fast: 150ms cubic-bezier(0.4, 0, 0.2, 1);
  --transition-normal: 300ms cubic-bezier(0.4, 0, 0.2, 1);
}
```

## Implementation Notes
When constructing panels, use the glassmorphic stack:
```css
.panel {
  background: var(--bg-panel);
  backdrop-filter: blur(12px);
  -webkit-backdrop-filter: blur(12px);
  border: var(--border-glass);
  border-radius: var(--radius-lg);
  box-shadow: var(--shadow-md);
}
```
