// Claude Code 2.1.326 native default palettes; exporter verifies against the installed theme resolver.
// Backgrounds are explicit white/black reference canvases, not a claim about custom terminal settings.
export const HOST_THEMES = {
  "light": {
    "background": "rgb(255,255,255)",
    "foreground": {
      "inactive": "rgb(102,102,102)",
      "subtle": "rgb(175,175,175)",
      "merged": "rgb(135,0,255)",
      "warning": "rgb(150,108,30)",
      "error": "rgb(171,43,63)",
      "success": "rgb(44,122,57)"
    }
  },
  "dark": {
    "background": "rgb(0,0,0)",
    "foreground": {
      "inactive": "rgb(153,153,153)",
      "subtle": "rgb(80,80,80)",
      "merged": "rgb(175,135,255)",
      "warning": "rgb(255,193,7)",
      "error": "rgb(255,107,128)",
      "success": "rgb(78,186,101)"
    }
  }
} as const
