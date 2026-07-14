import { defineConfig } from "@pandacss/dev";

export default defineConfig({
  preflight: false,
  include: ["./src/**/*.{svelte,ts}"],
  exclude: [],
  outdir: "styled-system",
  theme: {
    extend: {
      tokens: {
        radii: {
          sm: { value: "7px" }
        },
        spacing: {
          3: { value: "12px" },
          7: { value: "28px" }
        },
        durations: {
          quick: { value: "140ms" },
          smooth: { value: "180ms" }
        },
        easings: {
          standard: { value: "ease" }
        }
      },
      semanticTokens: {
        colors: {
          accent: { value: "var(--accent)" },
          amber: { value: "var(--amber)" },
          danger: { value: "var(--danger)" },
          dangerText: { value: "var(--danger-text)" },
          warnText: { value: "var(--warn-text)" }
        }
      },
      recipes: {
        appShellRecipe: {
          className: "cs-app-shell",
          description: "Application shell grid.",
          base: {
            display: "grid",
            gridTemplateColumns: "244px minmax(0, 1fr)",
            height: "100vh",
            minHeight: 0,
            overflow: "hidden",
            background: "var(--bg)",
            "@media (max-width: 900px)": {
              gridTemplateColumns: "1fr",
              gridTemplateRows: "auto minmax(0, 1fr)"
            }
          }
        },
        appSidebarRecipe: {
          className: "cs-app-sidebar",
          description: "Application sidebar shell.",
          base: {
            display: "flex",
            flexDirection: "column",
            minHeight: 0,
            height: "100vh",
            overflow: "hidden",
            borderRight: "1px solid var(--border)",
            padding: "18px 12px 12px",
            background: "var(--sidebar-bg)",
            color: "var(--text)",
            "@media (max-width: 900px)": {
              position: "relative",
              zIndex: 2,
              height: "auto",
              maxHeight: "none",
              borderRight: 0,
              borderBottom: "1px solid var(--border)",
              padding: "10px 10px 8px"
            }
          }
        },
        appBrandRecipe: {
          className: "cs-app-brand",
          description: "Application brand block.",
          base: {
            display: "grid",
            gridTemplateColumns: "34px minmax(0, 1fr)",
            alignItems: "center",
            gap: "10px",
            padding: "0 8px 18px",
            "& strong": {
              display: "block",
              minWidth: 0,
              overflow: "hidden",
              color: "var(--text)",
              fontSize: "15px",
              fontWeight: "700",
              lineHeight: "1.2",
              textOverflow: "ellipsis",
              whiteSpace: "nowrap"
            },
            "@media (max-width: 900px)": {
              paddingBottom: "10px"
            }
          }
        },
        appBrandMarkRecipe: {
          className: "cs-app-brand-mark",
          description: "Application sidebar brand mark.",
          base: {
            display: "grid",
            placeItems: "center",
            width: "34px",
            height: "34px",
            borderRadius: "7px",
            background: "transparent",
            color: "var(--brand-icon-ink)",
            "& .brand-logo": {
              display: "block",
              width: "100%",
              height: "100%"
            }
          }
        },
        appNavRecipe: {
          className: "cs-app-nav",
          description: "Application sidebar navigation list.",
          base: {
            display: "grid",
            gap: "8px",
            minHeight: 0,
            overflow: "auto",
            padding: "6px 0",
            "@media (max-width: 900px)": {
              gridTemplateColumns: "repeat(4, minmax(0, 1fr))",
              overflow: "visible"
            }
          }
        },
        appNavButtonRecipe: {
          className: "cs-app-nav-button",
          description: "Application navigation button.",
          base: {
            position: "relative",
            display: "flex",
            alignItems: "center",
            gap: "12px",
            width: "100%",
            minHeight: "42px",
            borderRadius: "7px",
            padding: "0 12px",
            background: "transparent",
            color: "var(--nav-icon)",
            textAlign: "left",
            transition: "background var(--motion-quick), color var(--motion-quick)",
            "&::before": {
              position: "absolute",
              inset: "8px 6px 8px auto",
              width: "3px",
              borderRadius: "999px",
              background: "var(--accent)",
              content: "\"\"",
              opacity: 0,
              transform: "scaleY(0.3)",
              transition: "opacity var(--motion-quick), transform var(--motion-spring)"
            },
            _hover: {
              background: "var(--surface-soft)",
              color: "var(--nav-icon-hover)"
            },
            "&[data-active='true']": {
              background: "var(--surface-hover)",
              color: "var(--nav-icon-hover)"
            },
            "&[data-active='true']::before": {
              opacity: 1,
              transform: "scaleY(1)"
            },
            "&[data-active='true'] svg": {
              color: "var(--nav-icon-hover)"
            },
            "@media (max-width: 900px)": {
              justifyContent: "center",
              minHeight: "42px",
              padding: "0 6px"
            },
            "@media (prefers-reduced-motion: reduce)": {
              transition: "none",
              transform: "none !important",
              "&::before": {
                transition: "none",
                transform: "none !important"
              },
              _hover: {
                transform: "none"
              },
              "&[data-active='true']": {
                transform: "none"
              }
            }
          }
        },
        appNavLabelRecipe: {
          className: "cs-app-nav-label",
          description: "Application navigation item label.",
          base: {
            minWidth: 0,
            flex: 1,
            overflow: "hidden",
            color: "var(--nav-text)",
            fontSize: "14px",
            fontWeight: "700",
            lineHeight: "1.2",
            textOverflow: "ellipsis",
            whiteSpace: "nowrap",
            ".cs-app-nav-button:hover &": {
              color: "var(--nav-text-hover)"
            },
            ".cs-app-nav-button[data-active='true'] &": {
              color: "var(--nav-text-hover)"
            },
            "@media (max-width: 900px)": {
              display: "none"
            }
          }
        },
        appNavUpdateDotRecipe: {
          className: "cs-app-nav-update-dot",
          description: "Application navigation update indicator.",
          base: {
            width: "7px",
            height: "7px",
            flex: "0 0 auto",
            borderRadius: "999px",
            background: "var(--accent)",
            boxShadow: "0 0 0 4px color-mix(in srgb, var(--accent) 12%, transparent)"
          }
        },
        appWorkspaceRecipe: {
          className: "cs-app-workspace",
          description: "Application workspace scroll container.",
          base: {
            minWidth: 0,
            minHeight: 0,
            height: "100vh",
            overflow: "auto",
            borderLeft: "1px solid var(--workspace-border)",
            background: "var(--bg)",
            padding: "var(--space-xl) var(--space-xl) 32px",
            "@media (max-width: 900px)": {
              height: "auto",
              padding: "14px"
            }
          }
        },
        appErrorBannerRecipe: {
          className: "cs-app-error-banner",
          description: "Top-level application error banner.",
          base: {
            boxSizing: "border-box",
            width: "100%",
            maxWidth: "1180px",
            margin: "0 auto var(--space-md)",
            border: "1px solid color-mix(in srgb, var(--danger) 34%, transparent)",
            borderRadius: "7px",
            background: "color-mix(in srgb, var(--danger) 13%, transparent)",
            color: "var(--danger-text)",
            padding: "10px 12px",
            fontSize: "13px",
            fontWeight: "800",
            overflowWrap: "anywhere"
          }
        },
        appRouteTransitionRecipe: {
          className: "cs-app-route-transition",
          description: "Application route transition wrapper.",
          base: {
            width: "100%",
            minWidth: 0,
            minHeight: 0,
            height: "100%",
            "& .cs-top-strip, & .cs-panel, & .cs-tool-card": {
              animation: "surface-rise 360ms cubic-bezier(0.16, 1, 0.3, 1) backwards",
              animationDelay: "calc(min(var(--surface-index, 0), 8) * 28ms)"
            },
            "& .cs-dashboard-grid > article:nth-child(1)": {
              "--surface-index": "1"
            },
            "& .cs-dashboard-grid > article:nth-child(2)": {
              "--surface-index": "2"
            },
            "& .cs-dashboard-grid > article:nth-child(3)": {
              "--surface-index": "3"
            },
            "& .cs-dashboard-grid > article:nth-child(4)": {
              "--surface-index": "4"
            },
            "& .cs-dashboard-grid > article:nth-child(5)": {
              "--surface-index": "5"
            },
            "& .cs-dashboard-grid > article:nth-child(n + 6)": {
              "--surface-index": "6"
            },
            "@media (prefers-reduced-motion: reduce)": {
              animation: "none",
              transition: "none",
              transform: "none !important",
              "& .cs-top-strip, & .cs-panel, & .cs-tool-card": {
                animation: "none",
                transition: "none",
                transform: "none !important"
              }
            }
          }
        },
        noticeRecipe: {
          className: "notice",
          description: "Inline dismissible status notice.",
          base: {
            boxSizing: "border-box",
            display: "flex",
            alignItems: "center",
            justifyContent: "space-between",
            gap: "12px",
            alignSelf: "stretch",
            justifySelf: "stretch",
            marginInline: "0",
            maxWidth: "none",
            width: "100%",
            borderRadius: "7px",
            padding: "10px 12px",
            fontSize: "13px",
            fontWeight: "800"
          },
          variants: {
            tone: {
              success: {
                border: "1px solid color-mix(in srgb, var(--accent) 34%, transparent)",
                background: "color-mix(in srgb, var(--accent) 12%, transparent)",
                color: "var(--accent)"
              },
              error: {
                border: "1px solid color-mix(in srgb, var(--danger) 34%, transparent)",
                background: "color-mix(in srgb, var(--danger) 13%, transparent)",
                color: "var(--danger-text)"
              }
            }
          },
          defaultVariants: {
            tone: "success"
          }
        },
        statusPillRecipe: {
          className: "cs-status-pill",
          description: "Compact status pill used across diagnostic cards.",
          base: {
            display: "inline-flex",
            alignItems: "center",
            gap: "5px",
            minHeight: "24px",
            borderRadius: "999px",
            padding: "0 9px",
            fontSize: "12px",
            fontWeight: "800",
            whiteSpace: "nowrap"
          },
          variants: {
            tone: {
              good: {
                background: "color-mix(in srgb, var(--accent) 14%, transparent)",
                color: "var(--accent)"
              },
              bad: {
                background: "color-mix(in srgb, var(--danger) 16%, transparent)",
                color: "var(--danger-text)"
              },
              warn: {
                background: "color-mix(in srgb, var(--amber) 16%, transparent)",
                color: "var(--warn-text)"
              },
              info: {
                background: "color-mix(in srgb, var(--accent) 12%, transparent)",
                color: "var(--accent)"
              }
            }
          },
          defaultVariants: {
            tone: "info"
          }
        },
        secretInputRecipe: {
          className: "cs-secret-input",
          description: "Two-column API key input with a reveal button.",
          base: {
            display: "grid",
            gridTemplateColumns: "minmax(0, 1fr) 40px",
            gap: "8px"
          }
        },
        routeStackRecipe: {
          className: "cs-route-stack",
          description: "Shared route content stack.",
          base: {
            display: "grid",
            gap: "var(--space-lg)",
            width: "100%",
            maxWidth: "1120px",
            minWidth: 0
          },
          variants: {
            width: {
              default: {},
              desktopClient: {
                maxWidth: "1120px"
              },
              full: {
                maxWidth: "1120px"
              }
            }
          },
          defaultVariants: {
            width: "default"
          }
        },
        topStripRecipe: {
          className: "cs-top-strip",
          description: "Shared route top strip.",
          base: {
            position: "relative",
            display: "flex",
            alignItems: "flex-end",
            justifyContent: "space-between",
            gap: "var(--space-lg)",
            minWidth: 0,
            overflow: "hidden",
            border: "1px solid var(--border)",
            borderRadius: "var(--radius)",
            background: "var(--surface)",
            padding: "var(--space-xl) var(--space-xl) 20px",
            transition:
              "border-color var(--motion-quick), background var(--motion-quick), box-shadow var(--motion-smooth), transform var(--motion-spring)",
            "& > *": {
              position: "relative",
              zIndex: 1,
              minWidth: 0
            },
            "& h1": {
              margin: "9px 0 0",
              letterSpacing: 0,
              color: "var(--text)",
              fontSize: "26px",
              fontWeight: "700",
              lineHeight: "1.12"
            },
            "& p": {
              margin: "6px 0 0",
              color: "var(--text-muted)",
              lineHeight: "1.45"
            },
            "@media (max-width: 900px)": {
              display: "grid",
              gridTemplateColumns: "1fr",
              alignItems: "start",
              width: "100%"
            }
          },
          variants: {
            compact: {
              true: {
                alignItems: "center",
                padding: "16px var(--space-md)",
                "& h1": {
                  marginTop: "7px"
                }
              }
            }
          }
        },
        topActionsRecipe: {
          className: "cs-top-actions",
          description: "Shared route top action row.",
          base: {
            display: "flex",
            alignItems: "center",
            justifyContent: "flex-end",
            gap: "9px",
            flexWrap: "wrap",
            "@media (max-width: 860px)": {
              justifyContent: "flex-start",
              "& button": {
                flex: "1 1 150px"
              }
            }
          }
        },
        statusStripRecipe: {
          className: "cs-status-strip",
          description: "Shared route status pill strip.",
          base: {
            display: "flex",
            alignItems: "center",
            gap: "9px",
            flexWrap: "wrap",
            marginTop: "13px",
            color: "var(--text-muted)",
            fontSize: "13px"
          }
        },
        sectionActionsRecipe: {
          className: "cs-section-actions",
          description: "Shared section-level action row.",
          base: {
            display: "flex",
            alignItems: "center",
            justifyContent: "flex-end",
            gap: "9px",
            flexWrap: "wrap",
            minWidth: 0
          }
        },
        panelRecipe: {
          className: "cs-panel",
          description: "Shared bordered surface panel.",
          base: {
            minWidth: 0,
            border: "1px solid var(--border)",
            borderRadius: "var(--radius)",
            background: "var(--surface)",
            transition:
              "border-color var(--motion-quick), background var(--motion-quick), box-shadow var(--motion-smooth), transform var(--motion-spring)"
          }
        },
        sectionHeadingRecipe: {
          className: "cs-section-heading",
          description: "Panel header with title copy and trailing action/content.",
          base: {
            display: "flex",
            alignItems: "center",
            justifyContent: "space-between",
            gap: "var(--space-md)",
            boxShadow: "inset 0 -1px 0 var(--border)",
            padding: "16px var(--space-lg)",
            "& h2": {
              margin: 0,
              letterSpacing: 0,
              color: "var(--text)",
              fontSize: "16px",
              fontWeight: "700",
              lineHeight: "1.3"
            },
            "& p": {
              margin: "6px 0 0",
              color: "var(--text-muted)",
              lineHeight: "1.45"
            }
          },
          variants: {
            compact: {
              true: {
                paddingBottom: "var(--space-md)"
              }
            }
          }
        },
        actionButtonRecipe: {
          className: "cs-action-button",
          description: "Shared text button styles for primary and secondary actions.",
          base: {
            display: "inline-flex",
            alignItems: "center",
            justifyContent: "center",
            gap: "6px",
            minHeight: "32px",
            border: "1px solid transparent",
            borderRadius: "7px",
            padding: "0 10px",
            fontSize: "11px",
            fontWeight: "700",
            lineHeight: "1.2",
            whiteSpace: "nowrap",
            "& svg": {
              width: "17px",
              height: "17px"
            },
            "&[data-refresh-button='true']": {
              fontSize: "12px",
              fontWeight: "800",
              "& svg": {
                width: "15px",
                height: "15px"
              }
            },
            transition:
              "background var(--motion-quick), border-color var(--motion-quick), color var(--motion-quick), opacity var(--motion-quick), transform var(--motion-smooth), box-shadow var(--motion-smooth)",
            _disabled: {
              cursor: "not-allowed",
              opacity: 0.48
            }
          },
          variants: {
            tone: {
              primary: {
                borderColor: "color-mix(in srgb, var(--accent) 52%, transparent)",
                background: "var(--accent)",
                color: "var(--accent-ink)",
                _hover: {
                  background: "var(--accent-hover)",
                  transform: "translateY(-1px)",
                  boxShadow: "0 8px 18px color-mix(in srgb, var(--accent) 18%, transparent)"
                }
              },
              secondary: {
                borderColor: "var(--border)",
                background: "var(--surface-soft)",
                color: "var(--text)",
                _hover: {
                  borderColor: "var(--border-strong)",
                  background: "var(--surface-hover)",
                  transform: "translateY(-1px)",
                  boxShadow: "0 8px 18px color-mix(in srgb, var(--modal-shadow) 18%, transparent)"
                }
              }
            },
            compact: {
              true: {
                width: "auto",
                minWidth: 0,
                height: "auto",
                minHeight: "30px",
                padding: "0 9px",
                fontSize: "10.5px",
                lineHeight: "1.25"
              }
            }
          },
          defaultVariants: {
            tone: "secondary"
          }
        },
        iconButtonRecipe: {
          className: "cs-icon-button",
          description: "Shared square icon button.",
          base: {
            display: "inline-flex",
            alignItems: "center",
            justifyContent: "center",
            gap: "7px",
            width: "32px",
            minHeight: "32px",
            border: "1px solid transparent",
            borderRadius: "7px",
            background: "var(--surface-soft)",
            color: "var(--text-soft)",
            whiteSpace: "nowrap",
            transition:
              "background var(--motion-quick), border-color var(--motion-quick), color var(--motion-quick), opacity var(--motion-quick), transform var(--motion-smooth), box-shadow var(--motion-smooth)",
            _hover: {
              borderColor: "var(--border-strong)",
              background: "var(--surface-hover)",
              transform: "translateY(-1px)",
              boxShadow: "0 8px 18px color-mix(in srgb, var(--modal-shadow) 18%, transparent)"
            },
            _disabled: {
              cursor: "not-allowed",
              opacity: 0.48
            }
          },
          variants: {
            danger: {
              true: {
                color: "var(--danger)"
              }
            },
            compact: {
              true: {
                width: "28px",
                height: "28px",
                minHeight: "28px",
                padding: 0
              }
            }
          }
        },
        emptyRowRecipe: {
          className: "cs-empty-row",
          description: "Shared empty state row.",
          base: {
            padding: "18px",
            color: "var(--text-muted)",
            textAlign: "center"
          }
        },
        spinRecipe: {
          className: "cs-spin",
          description: "Shared loading icon spin animation.",
          base: {
            animation: "spin 0.9s linear infinite"
          }
        },
        toolIconRecipe: {
          className: "cs-tool-icon",
          description: "Shared AI/system tool icon frame with tone and size variants.",
          base: {
            display: "grid",
            flex: "0 0 40px",
            placeItems: "center",
            width: "40px",
            minWidth: "40px",
            height: "40px",
            minHeight: "40px",
            border: "1px solid var(--border)",
            borderRadius: "7px",
            background: "var(--surface-soft)",
            color: "var(--text)",
            "&[data-tool-icon-tone='codex']": {
              borderColor: "#111111",
              background: "#111111"
            },
            "&[data-tool-icon-tone='chatgpt-desktop-current'], &[data-tool-icon-tone='chatgpt-desktop-legacy']": {
              borderColor: "rgba(15, 23, 42, 0.1)",
              background: "#fff"
            },
            "&[data-tool-icon-tone='chatgpt-desktop-current'] img": {
              filter: "invert(1)"
            },
            "& img": {
              display: "block",
              width: "22px",
              height: "22px",
              objectFit: "contain"
            },
            "&[data-tool-icon-tone='gemini'] img, &[data-tool-icon-tone='openclaw'] img": {
              width: "30px",
              height: "30px",
              borderRadius: "7px"
            },
            "&[data-tool-icon-tone='vscode'] img": {
              width: "24px",
              height: "24px",
              borderRadius: "6px"
            },
            "&[data-tool-icon-tone='chatgpt-desktop-legacy'] img": {
              width: "32px",
              height: "32px",
              borderRadius: "7px"
            },
            "&[data-tool-icon-tone='hermes'] img": {
              width: "36px",
              height: "36px",
              borderRadius: "7px"
            },
            "&[data-tool-icon-tone='grok'] img": {
              width: "36px",
              height: "36px",
              borderRadius: "7px"
            },
            "&[data-tool-icon-tone='pi'] img": {
              width: "36px",
              height: "36px"
            },
            "& [data-tool-icon-fallback-text]": {
              fontSize: "11px",
              fontWeight: "800",
              lineHeight: "1"
            }
          },
          variants: {
            variant: {
              card: {},
              choice: {
                flexBasis: "28px",
                width: "28px",
                minWidth: "28px",
                height: "28px",
                minHeight: "28px",
                borderRadius: "6px",
                "& img": {
                  width: "18px",
                  height: "18px"
                },
                "&[data-tool-icon-tone='gemini'] img, &[data-tool-icon-tone='openclaw'] img": {
                  width: "23px",
                  height: "23px",
                  borderRadius: "5px"
                },
                "&[data-tool-icon-tone='vscode'] img": {
                  width: "18px",
                  height: "18px",
                  borderRadius: "4px"
                },
                "&[data-tool-icon-tone='chatgpt-desktop-legacy'] img": {
                  width: "24px",
                  height: "24px",
                  borderRadius: "5px"
                },
                "&[data-tool-icon-tone='hermes'] img": {
                  width: "26px",
                  height: "26px",
                  borderRadius: "5px"
                },
                "&[data-tool-icon-tone='grok'] img": {
                  width: "26px",
                  height: "26px",
                  borderRadius: "5px"
                },
                "&[data-tool-icon-tone='pi'] img": {
                  width: "28px",
                  height: "28px"
                }
              },
              heading: {
                flexBasis: "32px",
                width: "32px",
                minWidth: "32px",
                height: "32px",
                minHeight: "32px",
                "& img": {
                  width: "20px",
                  height: "20px"
                },
                "&[data-tool-icon-tone='gemini'] img, &[data-tool-icon-tone='openclaw'] img": {
                  width: "24px",
                  height: "24px"
                },
                "&[data-tool-icon-tone='vscode'] img": {
                  width: "20px",
                  height: "20px",
                  borderRadius: "5px"
                },
                "&[data-tool-icon-tone='chatgpt-desktop-legacy'] img": {
                  width: "26px",
                  height: "26px"
                },
                "&[data-tool-icon-tone='hermes'] img": {
                  width: "28px",
                  height: "28px"
                },
                "&[data-tool-icon-tone='grok'] img": {
                  width: "28px",
                  height: "28px"
                },
                "&[data-tool-icon-tone='pi'] img": {
                  width: "28px",
                  height: "28px"
                }
              }
            },
            tone: {
              default: {},
              light: {
                borderColor: "rgba(15, 23, 42, 0.1)",
                background: "#fff"
              },
              codex: {
                borderColor: "#111111",
                background: "#111111"
              },
              claude: {
                background: "color-mix(in srgb, #d97757 12%, var(--surface-soft))"
              },
              gemini: {
                background: "var(--surface-soft)"
              },
              openclaw: {
                background: "var(--surface-soft)"
              },
              vscode: {
                borderColor: "#007ACC",
                background: "#007ACC"
              },
              "chatgpt-desktop-current": {
                borderColor: "rgba(15, 23, 42, 0.1)",
                background: "#fff"
              },
              "chatgpt-desktop-legacy": {
                borderColor: "rgba(15, 23, 42, 0.1)",
                background: "#fff"
              },
              hermes: {
                borderColor: "rgba(15, 23, 42, 0.1)",
                background: "#fff"
              },
              grok: {
                borderColor: "#0A0A0A",
                background: "#0A0A0A"
              },
              pi: {
                borderColor: "color-mix(in srgb, var(--text) 14%, transparent)",
                background: "var(--surface-soft)"
              }
            }
          },
          defaultVariants: {
            variant: "card",
            tone: "default"
          }
        },
        problemListRecipe: {
          className: "cs-problem-list",
          description: "Grid list for dashboard problems.",
          base: {
            display: "grid",
            gap: "var(--space-sm)",
            padding: "var(--space-md)"
          }
        },
        problemRowRecipe: {
          className: "cs-problem-row",
          description: "Diagnostic problem row with status, copy, and action.",
          base: {
            display: "grid",
            gridTemplateColumns: "auto minmax(0, 1fr) auto",
            alignItems: "center",
            gap: "12px",
            padding: "12px",
            border: "1px solid var(--border)",
            borderRadius: "var(--radius)",
            background: "var(--surface-strong)",
            "& h3": {
              margin: 0,
              color: "var(--text)",
              fontSize: "14px",
              lineHeight: "1.25"
            },
            "& p": {
              margin: "6px 0 0",
              color: "var(--text-muted)",
              overflowWrap: "anywhere",
              fontSize: "13px",
              lineHeight: "1.45"
            },
            "@media (max-width: 860px)": {
              gridTemplateColumns: "1fr"
            }
          }
        },
        activityListRecipe: {
          className: "cs-activity-list",
          description: "Grid list for activity events.",
          base: {
            display: "grid",
            gap: "var(--space-sm)",
            padding: "var(--space-md)"
          }
        },
        activityRowRecipe: {
          className: "cs-activity-row",
          description: "Activity event row.",
          base: {
            display: "grid",
            gridTemplateColumns: "auto minmax(0, 1fr) auto",
            alignItems: "center",
            gap: "12px",
            padding: "12px",
            border: "1px solid var(--border)",
            borderRadius: "var(--radius)",
            background: "var(--surface-strong)",
            "& p": {
              margin: 0,
              color: "var(--text-soft)"
            },
            "& time": {
              color: "var(--text-muted)",
              fontSize: "12px"
            },
            "@media (max-width: 860px)": {
              gridTemplateColumns: "1fr"
            }
          }
        },
        dashboardGridRecipe: {
          className: "cs-dashboard-grid",
          description: "Dashboard responsive card grid.",
          base: {
            display: "grid",
            gridTemplateColumns: "repeat(auto-fit, minmax(min(100%, 300px), 1fr))",
            gap: "var(--space-md)",
            padding: "var(--space-md)",
            "& > [data-dashboard-empty]": {
              gridColumn: "1 / -1"
            },
            "& > article": {
              animation: "surface-rise 360ms cubic-bezier(0.16, 1, 0.3, 1) backwards",
              animationDelay: "calc(min(var(--surface-index, 0), 8) * 28ms)"
            },
            "& > article:nth-child(1)": {
              "--surface-index": "1"
            },
            "& > article:nth-child(2)": {
              "--surface-index": "2"
            },
            "& > article:nth-child(3)": {
              "--surface-index": "3"
            },
            "& > article:nth-child(4)": {
              "--surface-index": "4"
            },
            "& > article:nth-child(5)": {
              "--surface-index": "5"
            },
            "& > article:nth-child(n + 6)": {
              "--surface-index": "6"
            }
          },
          variants: {
            kind: {
              client: {},
              system: {}
            }
          }
        },
        dashboardCardRecipe: {
          className: "cs-dashboard-card",
          description: "Dashboard system and client card.",
          base: {
            position: "relative",
            display: "grid",
            gridTemplateColumns: "minmax(86px, auto) minmax(220px, 1fr)",
            gridTemplateRows: "minmax(0, 1fr) auto",
            alignItems: "stretch",
            gap: "10px",
            minWidth: 0,
            minHeight: "132px",
            padding: "14px",
            border: "1px solid var(--border)",
            borderRadius: "var(--radius)",
            background: "var(--surface-strong)",
            transition:
              "border-color var(--motion-quick), background var(--motion-quick), box-shadow var(--motion-smooth), transform var(--motion-spring)",
            willChange: "transform, box-shadow",
            _hover: {
              borderColor: "var(--border-strong)",
              background: "var(--tool-hover-bg)",
              transform: "translateY(-2px)",
              boxShadow: "0 10px 24px color-mix(in srgb, var(--modal-shadow) 26%, transparent)"
            },
            "&:has([data-dashboard-overflow][open])": {
              zIndex: 6
            },
            "@media (prefers-reduced-motion: reduce)": {
              animation: "none",
              transition: "none",
              transform: "none !important",
              _hover: {
                transform: "none"
              }
            },
            "@media (max-width: 860px)": {
              gridTemplateColumns: "1fr"
            }
          },
          variants: {
            clickable: {
              true: {
                cursor: "pointer"
              }
            }
          }
        },
        dashboardCardMainRecipe: {
          className: "cs-dashboard-card-main",
          description: "Dashboard card icon and copy cluster.",
          base: {
            display: "flex",
            alignItems: "flex-start",
            gap: "12px",
            gridColumn: "1 / -1",
            gridRow: "1",
            minWidth: 0,
            "& h3": {
              margin: 0,
              color: "var(--text)",
              fontSize: "14px",
              lineHeight: "1.25"
            },
            "& p": {
              margin: "6px 0 0",
              color: "var(--text-muted)",
              fontSize: "12px",
              lineHeight: "1.35",
              overflowWrap: "anywhere",
              whiteSpace: "normal"
            }
          }
        },
        dashboardCardStateRecipe: {
          className: "cs-dashboard-card-state",
          description: "Dashboard card status area.",
          base: {
            display: "flex",
            alignItems: "center",
            alignSelf: "end",
            justifyContent: "flex-start",
            gap: "6px",
            gridColumn: "1",
            gridRow: "2",
            minWidth: "86px",
            flexWrap: "nowrap"
          }
        },
        dashboardCardActionsRecipe: {
          className: "cs-dashboard-card-actions",
          description: "Dashboard card action row.",
          base: {
            display: "flex",
            alignSelf: "end",
            alignItems: "center",
            justifyContent: "flex-end",
            flexFlow: "row nowrap",
            position: "relative",
            gap: "6px",
            gridColumn: "2",
            gridRow: "2",
            zIndex: 2,
            minWidth: 0,
            "& > button, & [data-dashboard-overflow-menu] button": {
              flex: "0 0 auto",
              width: "auto",
              minWidth: 0,
              minHeight: "28px",
              padding: "0 8px",
              fontSize: "10.5px",
              lineHeight: "1.2",
              textAlign: "center",
              "& svg": {
                width: "16px",
                height: "16px"
              }
            },
            "@media (max-width: 860px)": {
              justifyContent: "flex-start"
            }
          }
        },
        dashboardOverflowRecipe: {
          className: "cs-dashboard-overflow",
          description: "Dashboard card overflow action menu.",
          base: {
            position: "relative",
            flex: "0 0 auto",
            "& > summary": {
              display: "inline-flex",
              alignItems: "center",
              justifyContent: "center",
              width: "32px",
              height: "28px",
              minHeight: "28px",
              padding: 0,
              cursor: "pointer",
              listStyle: "none",
              borderRadius: "7px"
            },
            "& > summary::-webkit-details-marker": {
              display: "none"
            },
            "& > summary::marker": {
              content: "\"\""
            },
            "& [data-dashboard-overflow-menu]": {
              display: "none"
            },
            "&[open] [data-dashboard-overflow-menu]": {
              display: "grid"
            }
          }
        },
        dashboardEnvConflictRecipe: {
          className: "cs-dashboard-env-conflict",
          description: "Dashboard environment conflict repair banner.",
          base: {
            boxSizing: "border-box",
            display: "flex",
            alignItems: "flex-start",
            justifyContent: "space-between",
            gap: "12px",
            width: "100%",
            border: "1px solid color-mix(in srgb, var(--danger) 34%, transparent)",
            borderRadius: "7px",
            background: "color-mix(in srgb, var(--danger) 13%, transparent)",
            color: "var(--danger-text)",
            padding: "10px 12px",
            fontSize: "13px",
            fontWeight: "800",
            "& > div": {
              display: "grid",
              gap: "6px",
              minWidth: 0
            },
            "& [data-dashboard-env-conflict-chips]": {
              display: "flex",
              flexWrap: "wrap",
              gap: "6px"
            },
            "& [data-dashboard-env-conflict-chips] code": {
              border: "1px solid var(--border-subtle)",
              borderRadius: "6px",
              background: "var(--surface)",
              color: "var(--text)",
              padding: "3px 6px",
              overflowWrap: "anywhere"
            },
            "@media (max-width: 900px)": {
              display: "grid"
            }
          }
        },
        dashboardModalBackdropRecipe: {
          className: "cs-dashboard-modal-backdrop",
          description: "Dashboard modal backdrop.",
          base: {
            position: "fixed",
            inset: 0,
            zIndex: 20,
            display: "grid",
            alignItems: "start",
            justifyItems: "center",
            padding: "20px",
            overflow: "auto",
            overscrollBehavior: "contain",
            background: "rgba(0, 0, 0, 0.58)",
            "@media (max-width: 900px)": {
              padding: "12px"
            }
          }
        },
        dashboardModalPanelRecipe: {
          className: "cs-dashboard-modal-panel",
          description: "Dashboard wide modal panel.",
          base: {
            display: "flex",
            flexDirection: "column",
            width: "min(760px, calc(100vw - 40px))",
            maxWidth: "calc(100vw - 40px)",
            maxHeight: "calc(100vh - 40px)",
            minHeight: 0,
            overflow: "hidden",
            overscrollBehavior: "contain",
            border: "1px solid var(--border-strong)",
            borderRadius: "var(--radius)",
            background: "var(--surface)",
            boxShadow: "0 28px 80px var(--modal-shadow)",
            "& > *": {
              minWidth: 0
            },
            "@media (max-width: 900px)": {
              width: "min(100%, calc(100vw - 24px))",
              maxWidth: "calc(100vw - 24px)",
              maxHeight: "calc(100vh - 24px)"
            },
            "@supports (width: 100dvw)": {
              width: "min(760px, calc(100dvw - 40px))",
              maxWidth: "calc(100dvw - 40px)",
              maxHeight: "calc(100dvh - 40px)",
              "@media (max-width: 900px)": {
                width: "min(100%, calc(100dvw - 24px))",
                maxWidth: "calc(100dvw - 24px)",
                maxHeight: "calc(100dvh - 24px)"
              }
            }
          }
        },
        dashboardModalBodyRecipe: {
          className: "cs-dashboard-modal-body",
          description: "Dashboard modal scrollable body.",
          base: {
            display: "grid",
            gap: "18px",
            flex: "1 1 auto",
            minHeight: 0,
            overflow: "auto",
            overscrollBehavior: "contain",
            padding: "20px",
            scrollbarGutter: "stable",
            "& h2": {
              margin: "6px 0 0",
              letterSpacing: 0,
              color: "var(--text)",
              fontSize: "22px"
            },
            "& p": {
              margin: "6px 0 0",
              color: "var(--text-muted)",
              lineHeight: "1.45"
            },
            "@media (max-width: 900px)": {
              gap: "14px",
              padding: "16px"
            }
          }
        },
        dashboardModalActionsRecipe: {
          className: "cs-dashboard-modal-actions",
          description: "Dashboard modal action footer.",
          base: {
            display: "flex",
            alignItems: "center",
            justifyContent: "flex-end",
            gap: "9px",
            flex: "0 0 auto",
            flexWrap: "wrap",
            borderTop: "1px solid var(--border)",
            padding: "12px 20px 20px",
            background: "color-mix(in srgb, var(--surface) 96%, transparent)",
            backdropFilter: "blur(10px)",
            "@media (max-width: 900px)": {
              padding: "10px 16px 16px"
            }
          }
        },
        dashboardProgressRecipe: {
          className: "cs-dashboard-progress",
          description: "Dashboard install or launch progress panel.",
          base: {
            display: "grid",
            gap: "10px",
            minWidth: 0,
            border: "1px solid var(--border)",
            borderRadius: "var(--radius)",
            background: "var(--surface-strong)",
            padding: "12px",
            "& [data-dashboard-progress-copy]": {
              display: "flex",
              alignItems: "center",
              justifyContent: "space-between",
              gap: "10px",
              minWidth: 0
            },
            "& [data-dashboard-progress-copy] strong": {
              color: "var(--text)",
              fontSize: "13px"
            },
            "& [data-dashboard-progress-copy] span": {
              color: "var(--text-muted)",
              fontSize: "12px",
              lineHeight: "1.35",
              overflowWrap: "anywhere",
              textAlign: "right"
            },
            "& [data-dashboard-progress-track]": {
              position: "relative",
              height: "7px",
              overflow: "hidden",
              borderRadius: "999px",
              background: "var(--progress-bg)"
            },
            "& [data-dashboard-progress-fill]": {
              display: "block",
              height: "100%",
              borderRadius: "inherit",
              background: "var(--accent)",
              transition: "width 0.18s ease"
            },
            "& [data-dashboard-progress-track][data-indeterminate='true'] [data-dashboard-progress-fill]": {
              animation: "progress-pulse 1.1s ease-in-out infinite alternate"
            },
            "@media (max-width: 900px)": {
              "& [data-dashboard-progress-copy]": {
                display: "grid",
                justifyContent: "stretch"
              },
              "& [data-dashboard-progress-copy] span": {
                textAlign: "left"
              }
            }
          }
        },
        dashboardCommandBoxRecipe: {
          className: "cs-dashboard-command-box",
          description: "Dashboard command preview panel.",
          base: {
            display: "grid",
            gridTemplateColumns: "minmax(0, 1fr) auto",
            gap: "10px",
            alignItems: "start",
            minWidth: 0,
            border: "1px solid var(--border)",
            borderRadius: "var(--radius)",
            background: "var(--surface-strong)",
            padding: "12px",
            "& > div": {
              gridColumn: "1 / -1",
              display: "grid",
              gap: "4px",
              minWidth: 0
            },
            "& strong": {
              color: "var(--text)",
              fontSize: "13px",
              lineHeight: "1.3"
            },
            "& span": {
              display: "inline-flex",
              alignItems: "center",
              gap: "8px",
              flexWrap: "wrap",
              color: "var(--text-muted)",
              fontSize: "12px",
              lineHeight: "1.35",
              overflowWrap: "anywhere"
            },
            "& code": {
              minWidth: 0
            }
          }
        },
        dashboardCommandListRecipe: {
          className: "cs-dashboard-command-list",
          description: "Dashboard staged command list.",
          base: {
            display: "grid",
            gap: "8px",
            minWidth: 0,
            "& > div": {
              display: "grid",
              gap: "5px",
              minWidth: 0
            },
            "& span": {
              display: "inline-flex",
              alignItems: "center",
              width: "fit-content",
              minHeight: "22px",
              borderRadius: "999px",
              background: "var(--surface-hover)",
              color: "var(--text-soft)",
              padding: "0 8px",
              fontSize: "12px",
              fontWeight: "800"
            }
          }
        },
        dashboardInfoGridRecipe: {
          className: "cs-dashboard-info-grid",
          description: "Dashboard modal small metadata/result grid.",
          base: {
            display: "grid",
            gridTemplateColumns: "repeat(auto-fit, minmax(min(100%, 180px), 1fr))",
            gap: "8px",
            "& > span, & > div": {
              border: "1px solid var(--border)",
              borderRadius: "7px",
              background: "var(--surface-strong)",
              padding: "10px 12px"
            },
            "& > span": {
              color: "var(--text-soft)",
              fontSize: "12px",
              fontWeight: "800",
              lineHeight: "1.35"
            },
            "& > div": {
              display: "grid",
              gap: "5px"
            },
            "& strong": {
              color: "var(--text)",
              fontSize: "13px",
              lineHeight: "1.3"
            },
            "& span": {
              color: "var(--text-muted)",
              fontSize: "12px",
              lineHeight: "1.35",
              overflowWrap: "anywhere"
            }
          }
        },
        dashboardPreviewListRecipe: {
          className: "cs-dashboard-preview-list",
          description: "Dashboard modal preview/result list.",
          base: {
            display: "grid",
            gap: "10px",
            "& div": {
              display: "grid",
              gap: "5px",
              border: "1px solid var(--border)",
              borderRadius: "var(--radius)",
              background: "var(--surface-strong)",
              padding: "10px 12px"
            },
            "& strong": {
              color: "var(--text)",
              fontSize: "13px"
            },
            "& span": {
              color: "var(--text-muted)",
              fontSize: "13px",
              lineHeight: "1.4",
              overflowWrap: "anywhere"
            }
          }
        },
        dashboardLogRecipe: {
          className: "cs-dashboard-log",
          description: "Dashboard console output log panel.",
          base: {
            display: "grid",
            gap: "8px",
            minWidth: 0,
            border: "1px solid var(--border)",
            borderRadius: "var(--radius)",
            background: "var(--surface-strong)",
            padding: "12px",
            "& strong": {
              color: "var(--text)",
              fontSize: "13px",
              lineHeight: "1.3"
            },
            "& pre": {
              maxHeight: "220px",
              margin: 0,
              overflow: "auto",
              border: "1px solid var(--border)",
              borderRadius: "6px",
              background: "var(--code-bg)",
              color: "var(--code-text)",
              padding: "10px",
              fontFamily: "ui-monospace, \"SFMono-Regular\", Consolas, monospace",
              fontSize: "12px",
              lineHeight: "1.45",
              whiteSpace: "pre-wrap",
              overflowWrap: "anywhere"
            }
          },
          variants: {
            live: {
              true: {
                minHeight: "160px"
              }
            }
          }
        },
        dashboardLogViewportRecipe: {
          className: "cs-dashboard-log-viewport",
          description: "Dashboard live log scroll viewport.",
          base: {
            display: "grid",
            gap: "10px",
            maxHeight: "280px",
            overflow: "auto",
            paddingRight: "4px",
            scrollBehavior: "smooth"
          }
        },
        dashboardLogStageRecipe: {
          className: "cs-dashboard-log-stage",
          description: "Dashboard log output stage block.",
          base: {
            display: "grid",
            gap: "7px",
            minWidth: 0,
            "& + &": {
              borderTop: "1px solid var(--border)",
              paddingTop: "10px"
            },
            "& span, & b": {
              color: "var(--text-muted)",
              fontSize: "12px",
              fontWeight: "650",
              lineHeight: "1.35"
            },
            "& code": {
              width: "100%"
            }
          }
        },
        dashboardTerminalCardRecipe: {
          className: "cs-dashboard-terminal-card",
          description: "Dashboard terminal card.",
          base: {
            display: "grid",
            gap: "10px",
            minWidth: 0,
            border: "1px solid var(--border)",
            borderRadius: "var(--radius)",
            background: "var(--surface-strong)",
            padding: "12px"
          }
        },
        dashboardTerminalHeaderRecipe: {
          className: "cs-dashboard-terminal-header",
          description: "Dashboard terminal header.",
          base: {
            display: "flex",
            alignItems: "center",
            justifyContent: "space-between",
            gap: "10px",
            minWidth: 0,
            "& strong": {
              color: "var(--text)",
              fontSize: "13px",
              lineHeight: "1.3"
            },
            "& span": {
              color: "var(--text-muted)",
              fontSize: "12px",
              fontWeight: "750",
              lineHeight: "1.35"
            }
          }
        },
        dashboardTerminalFrameRecipe: {
          className: "cs-dashboard-terminal-frame",
          description: "Dashboard xterm host frame.",
          base: {
            minHeight: "340px",
            overflow: "hidden",
            border: "1px solid var(--border-strong)",
            borderRadius: "8px",
            background: "#0f172a",
            padding: "8px",
            "& .xterm": {
              height: "100%"
            },
            "& .xterm-viewport": {
              borderRadius: "6px"
            }
          }
        },
        dashboardLaunchSectionRecipe: {
          className: "cs-dashboard-launch-section",
          description: "Dashboard launch option section.",
          base: {
            display: "grid",
            gap: "10px",
            minWidth: 0
          }
        },
        dashboardLaunchHeadingRecipe: {
          className: "cs-dashboard-launch-heading",
          description: "Dashboard launch option section heading.",
          base: {
            display: "flex",
            alignItems: "center",
            justifyContent: "space-between",
            gap: "12px",
            minWidth: 0,
            "& strong": {
              color: "var(--text)",
              fontSize: "13px",
              lineHeight: "1.3"
            },
            "& span": {
              color: "var(--text-muted)",
              fontSize: "12px",
              fontWeight: "700",
              lineHeight: "1.35",
              overflowWrap: "anywhere",
              textAlign: "right"
            }
          }
        },
        dashboardLaunchGridRecipe: {
          className: "cs-dashboard-launch-grid",
          description: "Dashboard launch options grid.",
          base: {
            display: "grid",
            gridTemplateColumns: "repeat(auto-fit, minmax(min(100%, 180px), 1fr))",
            gap: "10px",
            minWidth: 0
          },
          variants: {
            compact: {
              true: {
                gridTemplateColumns: "repeat(auto-fit, minmax(min(100%, 150px), 1fr))"
              }
            }
          }
        },
        dashboardLaunchOptionRecipe: {
          className: "cs-dashboard-launch-option",
          description: "Dashboard launch option tile.",
          base: {
            display: "grid",
            gap: "5px",
            minWidth: 0,
            minHeight: "74px",
            border: "1px solid var(--border)",
            borderRadius: "8px",
            background: "var(--surface-strong)",
            padding: "11px 12px",
            textAlign: "left",
            transition:
              "border-color var(--motion-quick), background var(--motion-quick), box-shadow var(--motion-smooth), transform var(--motion-spring)",
            _hover: {
              borderColor: "var(--border-strong)",
              background: "var(--surface-hover)",
              transform: "translateY(-2px)",
              boxShadow: "0 12px 26px color-mix(in srgb, var(--modal-shadow) 20%, transparent)"
            },
            _disabled: {
              cursor: "not-allowed",
              opacity: 0.54
            },
            "&[data-selected='true']": {
              borderColor: "color-mix(in srgb, var(--accent) 48%, transparent)",
              background: "color-mix(in srgb, var(--accent) 10%, var(--surface-strong))",
              boxShadow: "0 0 0 1px color-mix(in srgb, var(--accent) 14%, transparent)"
            },
            "& strong": {
              color: "var(--text)",
              fontSize: "13px",
              lineHeight: "1.3",
              overflowWrap: "anywhere"
            },
            "& span": {
              color: "var(--text-muted)",
              fontSize: "12px",
              lineHeight: "1.35",
              overflowWrap: "anywhere"
            }
          },
          variants: {
            selected: {
              true: {
                borderColor: "color-mix(in srgb, var(--accent) 48%, transparent)",
                background: "color-mix(in srgb, var(--accent) 10%, var(--surface-strong))",
                boxShadow: "0 0 0 1px color-mix(in srgb, var(--accent) 14%, transparent)"
              }
            }
          }
        },
        dashboardLaunchEmptyRecipe: {
          className: "cs-dashboard-launch-empty",
          description: "Dashboard empty launch option tile.",
          base: {
            display: "grid",
            alignContent: "center",
            gap: "5px",
            minWidth: 0,
            minHeight: "74px",
            border: "1px solid var(--border)",
            borderRadius: "8px",
            background: "var(--surface-strong)",
            color: "var(--text-muted)",
            padding: "11px 12px",
            fontSize: "13px",
            lineHeight: "1.3",
            overflowWrap: "anywhere"
          }
        },
        dashboardDirectoryFieldRecipe: {
          className: "cs-dashboard-directory-field",
          description: "Dashboard launch working directory field.",
          base: {
            display: "grid",
            gap: "7px",
            minWidth: 0,
            color: "var(--text-soft)",
            fontSize: "13px",
            fontWeight: "700",
            "& input": {
              fontFamily: "ui-monospace, \"SFMono-Regular\", Consolas, monospace",
              fontSize: "12px",
              fontWeight: "500"
            }
          }
        },
        gatewayHeroRecipe: {
          className: "cs-gateway-hero",
          description: "Gateway top strip state accent.",
          base: {},
          variants: {
            tone: {
              offline: {},
              online: {
                borderColor: "color-mix(in srgb, var(--accent) 34%, var(--border))"
              }
            }
          },
          defaultVariants: {
            tone: "offline"
          }
        },
        gatewayPanelRecipe: {
          className: "cs-gateway-panel",
          description: "Gateway status/settings panel layout.",
          base: {
            display: "grid",
            gap: "var(--space-md)",
            padding: "var(--space-md)"
          }
        },
        gatewayMetricsRecipe: {
          className: "cs-gateway-metrics",
          description: "Gateway status metrics grid.",
          base: {
            display: "grid",
            gridTemplateColumns: "repeat(auto-fit, minmax(min(100%, 220px), 1fr))",
            gap: "var(--space-sm)",
            "& > div": {
              display: "grid",
              gap: "7px",
              minWidth: 0,
              border: "1px solid var(--border)",
              borderRadius: "var(--radius)",
              background: "var(--surface-strong)",
              padding: "13px"
            },
            "& span": {
              color: "var(--text-muted)",
              fontFamily: "ui-monospace, \"SFMono-Regular\", Consolas, monospace",
              fontSize: "11px",
              fontWeight: "800",
              textTransform: "uppercase"
            },
            "& strong": {
              minWidth: 0,
              overflowWrap: "anywhere",
              color: "var(--text)",
              fontSize: "14px",
              lineHeight: "1.25"
            },
            "& small": {
              color: "var(--text-muted)",
              fontSize: "12px",
              lineHeight: "1.35",
              overflowWrap: "anywhere"
            },
            "& code": {
              border: "1px solid var(--border)",
              borderRadius: "6px",
              padding: "6px 8px",
              background: "var(--code-bg)",
              color: "var(--code-text)",
              fontFamily: "ui-monospace, \"SFMono-Regular\", Consolas, monospace",
              fontSize: "12px",
              lineHeight: "1.45",
              overflowWrap: "anywhere",
              whiteSpace: "normal"
            }
          }
        },
        gatewaySettingRowRecipe: {
          className: "cs-gateway-setting-row",
          description: "Gateway privacy filter setting row.",
          base: {
            display: "grid",
            gridTemplateColumns: "minmax(120px, max-content) minmax(0, 1fr)",
            alignItems: "center",
            gap: "12px",
            padding: "4px 0 2px",
            "& > span": {
              color: "var(--text-muted)",
              fontSize: "12px",
              fontWeight: "800"
            },
            "@media (max-width: 860px)": {
              gridTemplateColumns: "minmax(0, 1fr)"
            }
          }
        },
        gatewaySegmentedRecipe: {
          className: "cs-gateway-segmented",
          description: "Gateway privacy filter segmented control.",
          base: {
            display: "grid",
            gridTemplateColumns: "repeat(4, minmax(0, 1fr))",
            gap: "6px",
            minWidth: 0,
            "& button": {
              minWidth: 0,
              minHeight: "30px",
              border: "1px solid var(--border)",
              borderRadius: "7px",
              background: "var(--surface-strong)",
              color: "var(--text-soft)",
              fontSize: "10.5px",
              fontWeight: "800",
              transition: "background var(--motion-quick), border-color var(--motion-quick), color var(--motion-quick)"
            },
            "& button:hover:not(:disabled)": {
              borderColor: "var(--border-strong)",
              background: "var(--surface-hover)",
              color: "var(--text)"
            },
            "& button[data-selected='true']": {
              borderColor: "color-mix(in srgb, var(--accent) 45%, transparent)",
              background: "color-mix(in srgb, var(--accent) 10%, var(--surface-strong))",
              color: "var(--accent)"
            },
            "@media (max-width: 860px)": {
              gridTemplateColumns: "repeat(2, minmax(0, 1fr))"
            }
          }
        },
        gatewayInlineErrorRecipe: {
          className: "cs-gateway-inline-error",
          description: "Gateway inline error banner.",
          base: {
            border: "1px solid color-mix(in srgb, var(--danger) 28%, transparent)",
            borderRadius: "7px",
            padding: "8px",
            background: "color-mix(in srgb, var(--danger) 10%, transparent)",
            color: "var(--danger-text)",
            fontSize: "11px",
            lineHeight: "1.35",
            overflowWrap: "anywhere"
          }
        },
        gatewayRequestPanelRecipe: {
          className: "cs-gateway-request-panel",
          description: "Gateway recent request log panel layout.",
          base: {
            display: "grid",
            gap: "12px",
            padding: "var(--space-md)"
          }
        },
        gatewayRequestListRecipe: {
          className: "cs-gateway-request-list",
          description: "Gateway recent request rows.",
          base: {
            display: "grid",
            gap: "8px"
          }
        },
        gatewayRequestRowRecipe: {
          className: "cs-gateway-request-row",
          description: "Gateway recent request row.",
          base: {
            display: "grid",
            gridTemplateColumns: "minmax(160px, 1.6fr) auto auto auto auto",
            alignItems: "center",
            gap: "var(--space-sm)",
            minWidth: 0,
            border: "1px solid var(--border)",
            borderRadius: "7px",
            background: "var(--surface-strong)",
            padding: "var(--space-sm)",
            "& strong, & small": {
              display: "block",
              minWidth: 0,
              overflow: "hidden",
              textOverflow: "ellipsis",
              whiteSpace: "nowrap"
            },
            "& strong": {
              color: "var(--text)",
              fontSize: "13px",
              lineHeight: "1.25"
            },
            "& small, & span": {
              color: "var(--text-muted)",
              fontSize: "12px",
              fontWeight: "700"
            },
            "& em": {
              justifySelf: "end",
              borderRadius: "999px",
              background: "var(--surface-hover)",
              color: "var(--text-soft)",
              padding: "4px 8px",
              fontSize: "12px",
              fontStyle: "normal",
              fontWeight: "800",
              whiteSpace: "nowrap"
            },
            "&[data-privacy-action='detected'] em": {
              background: "color-mix(in srgb, var(--amber) 14%, transparent)",
              color: "var(--warn-text)"
            },
            "&[data-privacy-action='redacted'] em": {
              background: "color-mix(in srgb, var(--accent) 12%, transparent)",
              color: "var(--accent)"
            },
            "&[data-privacy-action='blocked'] em": {
              background: "color-mix(in srgb, var(--danger) 12%, transparent)",
              color: "var(--danger-text)"
            },
            "& [data-gateway-request-time]": {
              minWidth: 0
            },
            "@media (max-width: 860px)": {
              gridTemplateColumns: "minmax(0, 1fr)",
              "& em": {
                justifySelf: "flex-start"
              },
              "& [data-gateway-request-time]": {
                display: "none"
              }
            }
          }
        },
        settingsListRecipe: {
          className: "cs-settings-list",
          description: "Settings route preference list panel.",
          base: {
            display: "grid",
            gap: "12px",
            padding: "var(--space-md)"
          }
        },
        settingsRowRecipe: {
          className: "cs-settings-row",
          description: "Settings route label/value row.",
          base: {
            display: "grid",
            gridTemplateColumns: "minmax(0, 1fr) auto",
            alignItems: "center",
            gap: "12px",
            minWidth: 0,
            border: "1px solid var(--border)",
            borderRadius: "var(--radius)",
            background: "var(--surface-strong)",
            padding: "12px",
            "& > span:first-child": {
              display: "flex",
              alignItems: "center",
              gap: "10px",
              minWidth: 0,
              color: "var(--text-soft)",
              overflowWrap: "anywhere"
            },
            "& a": {
              textDecoration: "none"
            },
            "@media (max-width: 860px)": {
              gridTemplateColumns: "minmax(0, 1fr)",
              justifyItems: "stretch",
              "& > select, & > button, & > a": {
                justifySelf: "start"
              }
            }
          }
        },
        settingsRowValueRecipe: {
          className: "cs-settings-row-value",
          description: "Settings route trailing row value.",
          base: {
            display: "grid",
            justifyItems: "end",
            gap: "5px",
            minWidth: 0,
            textAlign: "right",
            "& small": {
              maxWidth: "min(420px, 52vw)",
              color: "var(--text-muted)",
              lineHeight: "1.35"
            },
            "& input[type='checkbox']": {
              width: "18px",
              height: "18px",
              minHeight: "18px",
              padding: 0,
              accentColor: "var(--accent)"
            },
            "@media (max-width: 860px)": {
              justifyItems: "start",
              textAlign: "left"
            }
          }
        },
        settingsAboutPanelRecipe: {
          className: "cs-settings-about-panel",
          description: "Settings route about panel shell.",
          base: {
            overflow: "hidden"
          }
        },
        settingsAboutContentRecipe: {
          className: "cs-settings-about-content",
          description: "Settings route about panel content grid.",
          base: {
            display: "grid",
            gap: "var(--space-sm)",
            padding: "var(--space-md)"
          }
        },
        settingsAboutSummaryRecipe: {
          className: "cs-settings-about-summary",
          description: "Settings route app identity and update summary.",
          base: {
            display: "flex",
            alignItems: "center",
            gap: "12px",
            flexWrap: "wrap",
            minWidth: 0,
            border: "1px solid var(--border)",
            borderRadius: "var(--radius)",
            background: "var(--surface-strong)",
            padding: "12px"
          }
        },
        settingsAboutMarkRecipe: {
          className: "cs-settings-about-mark",
          description: "Settings route app logo mark.",
          base: {
            display: "grid",
            placeItems: "center",
            width: "40px",
            height: "40px",
            borderRadius: "7px",
            background: "var(--brand-icon-bg)",
            color: "var(--brand-icon-ink)",
            "& .brand-logo": {
              display: "block",
              width: "100%",
              height: "100%"
            }
          }
        },
        settingsAboutTitleRecipe: {
          className: "cs-settings-about-title",
          description: "Settings route app name/version copy.",
          base: {
            display: "grid",
            gap: "4px",
            minWidth: 0,
            flex: "1 1 220px",
            "& strong": {
              color: "var(--text)",
              fontSize: "16px",
              lineHeight: "1.2"
            },
            "& span": {
              color: "var(--text-muted)",
              fontFamily: "ui-monospace, \"SFMono-Regular\", Consolas, monospace",
              fontSize: "11px",
              lineHeight: "1.4"
            }
          }
        },
        settingsAboutUpdateRecipe: {
          className: "cs-settings-about-update",
          description: "Settings route app update action row.",
          base: {
            display: "flex",
            alignItems: "center",
            justifyContent: "flex-end",
            gap: "8px",
            flexWrap: "wrap",
            marginLeft: "auto",
            minWidth: 0,
            "@media (max-width: 860px)": {
              justifyContent: "flex-start",
              marginLeft: 0
            }
          }
        },
        settingsUpdatePillRecipe: {
          className: "cs-settings-update-pill",
          description: "Settings route application update status pill.",
          base: {
            display: "inline-flex",
            alignItems: "center",
            gap: "5px",
            maxWidth: "260px",
            minHeight: "36px",
            border: "1px solid var(--border)",
            borderRadius: "7px",
            padding: "0 10px",
            fontSize: "12px",
            fontWeight: "800",
            overflowWrap: "anywhere",
            lineHeight: "1.3"
          },
          variants: {
            tone: {
              good: {
                background: "color-mix(in srgb, var(--accent) 14%, transparent)",
                color: "var(--accent)"
              },
              bad: {
                background: "color-mix(in srgb, var(--danger) 16%, transparent)",
                color: "var(--danger-text)"
              },
              warn: {
                background: "color-mix(in srgb, var(--amber) 16%, transparent)",
                color: "var(--warn-text)"
              },
              info: {
                background: "color-mix(in srgb, var(--accent) 12%, transparent)",
                color: "var(--accent)"
              }
            }
          },
          defaultVariants: {
            tone: "info"
          }
        },
        terminalPanelRecipe: {
          className: "cs-terminal-panel",
          description: "Embedded terminal route shell.",
          base: {
            display: "grid",
            gridTemplateRows: "auto minmax(0, 1fr)",
            gap: "14px",
            minHeight: 0,
            height: "100%",
            overflow: "hidden"
          }
        },
        terminalPanelHeaderRecipe: {
          className: "cs-terminal-panel-header",
          description: "Embedded terminal header row.",
          base: {
            display: "flex",
            alignItems: "center",
            justifyContent: "space-between",
            gap: "12px",
            flexWrap: "wrap"
          }
        },
        terminalPanelTitleRecipe: {
          className: "cs-terminal-panel-title",
          description: "Embedded terminal title cluster.",
          base: {
            display: "flex",
            alignItems: "center",
            gap: "8px",
            minWidth: 0,
            color: "var(--text)",
            "& strong": {
              minWidth: 0,
              fontSize: "15px",
              lineHeight: "1.3",
              overflowWrap: "anywhere"
            }
          }
        },
        terminalPanelStatusRecipe: {
          className: "cs-terminal-panel-status",
          description: "Embedded terminal status text.",
          base: {
            display: "flex",
            alignItems: "center",
            gap: "8px",
            minWidth: 0,
            fontSize: "12px",
            fontWeight: "750",
            lineHeight: "1.35",
            overflowWrap: "anywhere"
          },
          variants: {
            tone: {
              running: {
                color: "#22c55e"
              },
              exited: {
                color: "var(--text-muted)"
              },
              idle: {
                color: "var(--text-muted)"
              },
              error: {
                color: "var(--danger, #ef4444)"
              }
            }
          },
          defaultVariants: {
            tone: "idle"
          }
        },
        terminalPanelActionsRecipe: {
          className: "cs-terminal-panel-actions",
          description: "Embedded terminal action buttons.",
          base: {
            display: "flex",
            alignItems: "center",
            gap: "8px",
            flexWrap: "wrap"
          }
        },
        terminalPanelFrameRecipe: {
          className: "cs-terminal-panel-frame",
          description: "Embedded xterm frame.",
          base: {
            minHeight: 0,
            height: "100%",
            overflow: "hidden",
            border: "1px solid var(--border-strong)",
            borderRadius: "10px",
            background: "#0f172a",
            padding: "8px",
            "& .xterm": {
              height: "100%"
            },
            "& .xterm-viewport": {
              borderRadius: "8px"
            }
          }
        },
        desktopClientTabsRecipe: {
          className: "cs-desktop-client-tabs",
          description: "Desktop client install-kind tab row.",
          base: {
            display: "flex",
            gap: "var(--space-sm)",
            flexWrap: "wrap",
            "& button": {
              display: "inline-flex",
              alignItems: "center",
              justifyContent: "center",
              gap: "6px",
              minHeight: "34px",
              border: "1px solid var(--border)",
              borderRadius: "8px",
              background: "var(--surface-strong)",
              color: "var(--text-soft)",
              padding: "0 10px",
              fontSize: "10.5px",
              fontWeight: "800",
              transition:
                "background var(--motion-quick), border-color var(--motion-quick), color var(--motion-quick)",
              _hover: {
                borderColor: "var(--border-strong)",
                background: "var(--surface-hover)",
                color: "var(--text)"
              }
            },
            "& button[data-selected='true']": {
              borderColor: "color-mix(in srgb, var(--accent) 45%, transparent)",
              background: "color-mix(in srgb, var(--accent) 10%, var(--surface-strong))",
              color: "var(--accent)"
            }
          }
        },
        desktopClientMetricsRecipe: {
          className: "cs-desktop-client-metrics",
          description: "Desktop client status metrics grid.",
          base: {
            display: "grid",
            gridTemplateColumns: "repeat(auto-fit, minmax(min(100%, 220px), 1fr))",
            gap: "var(--space-md)",
            padding: "var(--space-lg)",
            "& > div": {
              display: "grid",
              gap: "7px",
              minWidth: 0,
              border: "1px solid var(--border)",
              borderRadius: "var(--radius)",
              background: "var(--surface-strong)",
              padding: "var(--space-md)"
            },
            "& span": {
              color: "var(--text-muted)",
              fontFamily: "ui-monospace, \"SFMono-Regular\", Consolas, monospace",
              fontSize: "11px",
              fontWeight: "800",
              textTransform: "uppercase"
            },
            "& strong": {
              minWidth: 0,
              overflowWrap: "anywhere",
              color: "var(--text)",
              fontSize: "14px",
              lineHeight: "1.25"
            },
            "& small": {
              color: "var(--text-muted)",
              fontSize: "12px",
              lineHeight: "1.35",
              overflowWrap: "anywhere"
            }
          }
        },
        desktopClientActionsRecipe: {
          className: "cs-desktop-client-actions",
          description: "Desktop client install and update action row.",
          base: {
            display: "flex",
            alignItems: "center",
            gap: "var(--space-sm)",
            flexWrap: "wrap",
            padding: "0 var(--space-lg) var(--space-lg)",
            "& button": {
              minHeight: "33px"
            },
            "@media (max-width: 860px)": {
              justifyContent: "flex-start",
              "& button": {
                flex: "1 1 150px"
              }
            }
          }
        },
        desktopClientProgressRecipe: {
          className: "cs-desktop-client-progress",
          description: "Desktop client download/install progress panel.",
          base: {
            display: "grid",
            gap: "var(--space-sm)",
            margin: "0 var(--space-lg) var(--space-lg)",
            border: "1px solid var(--border)",
            borderRadius: "var(--radius)",
            background: "var(--surface-strong)",
            padding: "var(--space-md)",
            "& [data-desktop-client-progress-copy], & [data-desktop-client-progress-meta]": {
              display: "flex",
              alignItems: "center",
              justifyContent: "space-between",
              gap: "12px",
              minWidth: 0
            },
            "& [data-desktop-client-progress-copy] strong": {
              flex: "0 0 auto",
              color: "var(--text)",
              fontSize: "13px",
              lineHeight: "1.3"
            },
            "& [data-desktop-client-progress-copy] span, & [data-desktop-client-progress-meta] span": {
              minWidth: 0,
              color: "var(--text-muted)",
              fontSize: "12px",
              lineHeight: "1.35",
              overflowWrap: "anywhere"
            },
            "& [data-desktop-client-progress-copy] span": {
              textAlign: "right"
            },
            "& [data-desktop-client-progress-track]": {
              position: "relative",
              height: "7px",
              overflow: "hidden",
              borderRadius: "999px",
              background: "var(--progress-bg)"
            },
            "& [data-desktop-client-progress-fill]": {
              display: "block",
              height: "100%",
              borderRadius: "inherit",
              background: "var(--accent)",
              transition: "width 0.18s ease"
            },
            "& [data-desktop-client-progress-track][data-indeterminate='true'] [data-desktop-client-progress-fill]": {
              animation: "progress-pulse 1.1s ease-in-out infinite alternate"
            },
            "@media (max-width: 860px)": {
              "& [data-desktop-client-progress-copy], & [data-desktop-client-progress-meta]": {
                display: "grid",
                justifyContent: "stretch"
              },
              "& [data-desktop-client-progress-copy] span": {
                textAlign: "left"
              }
            }
          }
        },
        desktopClientPreviewListRecipe: {
          className: "cs-desktop-client-preview-list",
          description: "Desktop client update plan detail list.",
          base: {
            display: "grid",
            gap: "var(--space-md)",
            padding: "var(--space-lg)",
            "& > div": {
              display: "grid",
              gap: "5px",
              minWidth: 0,
              border: "1px solid var(--border)",
              borderRadius: "var(--radius)",
              background: "var(--surface-strong)",
              padding: "var(--space-md)"
            },
            "& strong": {
              color: "var(--text)",
              fontSize: "13px"
            },
            "& span": {
              color: "var(--text-muted)",
              fontSize: "13px",
              lineHeight: "1.4",
              overflowWrap: "anywhere"
            }
          }
        },
        desktopClientSettingsListRecipe: {
          className: "cs-desktop-client-settings-list",
          description: "Desktop client settings list.",
          base: {
            display: "grid",
            gap: "var(--space-md)",
            padding: "var(--space-lg)",
            "& > label:not([data-native-toggle])": {
              display: "grid",
              gap: "6px",
              minWidth: 0,
              color: "var(--text-soft)",
              fontSize: "13px",
              fontWeight: "800"
            },
            "& input:not([type='checkbox']), & select": {
              width: "100%",
              minHeight: "40px",
              border: "1px solid var(--border)",
              borderRadius: "7px",
              background: "var(--code-bg)",
              color: "var(--text)",
              padding: "0 10px"
            },
            "& select": {
              colorScheme: "var(--control-color-scheme)"
            }
          },
          variants: {
            layout: {
              stack: {},
              grid: {
                gridTemplateColumns: "repeat(auto-fit, minmax(220px, 1fr))"
              }
            }
          },
          defaultVariants: {
            layout: "stack"
          }
        },
        desktopClientModalBackdropRecipe: {
          className: "cs-desktop-client-modal-backdrop",
          description: "Desktop client modal backdrop.",
          base: {
            position: "fixed",
            inset: 0,
            zIndex: 20,
            display: "grid",
            alignItems: "start",
            justifyItems: "center",
            padding: "20px",
            overflow: "auto",
            overscrollBehavior: "contain",
            background: "rgba(0, 0, 0, 0.58)",
            "@media (max-width: 900px)": {
              padding: "12px"
            }
          }
        },
        desktopClientModalPanelRecipe: {
          className: "cs-desktop-client-modal-panel",
          description: "Desktop client modal panel.",
          base: {
            display: "flex",
            flexDirection: "column",
            width: "min(560px, 100%)",
            maxWidth: "calc(100vw - 40px)",
            maxHeight: "calc(100vh - 40px)",
            minHeight: 0,
            overflow: "hidden",
            overscrollBehavior: "contain",
            border: "1px solid var(--border-strong)",
            borderRadius: "var(--radius)",
            background: "var(--surface)",
            boxShadow: "0 28px 80px var(--modal-shadow)",
            "& > *": {
              minWidth: 0
            },
            "@media (max-width: 900px)": {
              width: "min(100%, calc(100vw - 24px))",
              maxWidth: "calc(100vw - 24px)",
              maxHeight: "calc(100vh - 24px)"
            },
            "@supports (width: 100dvw)": {
              maxWidth: "calc(100dvw - 40px)",
              maxHeight: "calc(100dvh - 40px)",
              "@media (max-width: 900px)": {
                width: "min(100%, calc(100dvw - 24px))",
                maxWidth: "calc(100dvw - 24px)",
                maxHeight: "calc(100dvh - 24px)"
              }
            }
          }
        },
        desktopClientModalBodyRecipe: {
          className: "cs-desktop-client-modal-body",
          description: "Desktop client modal scroll body.",
          base: {
            display: "grid",
            gap: "18px",
            flex: "1 1 auto",
            minHeight: 0,
            overflow: "auto",
            overscrollBehavior: "contain",
            padding: "20px",
            scrollbarGutter: "stable",
            "& h2": {
              margin: "6px 0 0",
              letterSpacing: 0,
              color: "var(--text)",
              fontSize: "22px"
            },
            "& p": {
              margin: "6px 0 0",
              color: "var(--text-muted)",
              lineHeight: "1.45"
            },
            "@media (max-width: 900px)": {
              gap: "14px",
              padding: "16px"
            }
          }
        },
        desktopClientModalActionsRecipe: {
          className: "cs-desktop-client-modal-actions",
          description: "Desktop client modal action footer.",
          base: {
            display: "flex",
            alignItems: "center",
            justifyContent: "flex-end",
            gap: "9px",
            flex: "0 0 auto",
            flexWrap: "wrap",
            borderTop: "1px solid var(--border)",
            padding: "12px 20px 20px",
            background: "color-mix(in srgb, var(--surface) 96%, transparent)",
            backdropFilter: "blur(10px)",
            "@media (max-width: 900px)": {
              padding: "10px 16px 16px"
            }
          }
        },
        desktopClientLogRecipe: {
          className: "cs-desktop-client-log",
          description: "Desktop client live install log panel.",
          base: {
            display: "grid",
            gap: "var(--space-sm)",
            minWidth: 0,
            margin: "var(--space-lg)",
            border: "1px solid var(--border)",
            borderRadius: "var(--radius)",
            background: "var(--surface-strong)",
            padding: "var(--space-md)",
            "& pre": {
              maxHeight: "220px",
              margin: 0,
              overflow: "auto",
              border: "1px solid var(--border)",
              borderRadius: "6px",
              background: "var(--code-bg)",
              color: "var(--code-text)",
              padding: "10px",
              fontFamily: "ui-monospace, \"SFMono-Regular\", Consolas, monospace",
              fontSize: "12px",
              lineHeight: "1.45",
              whiteSpace: "pre-wrap",
              overflowWrap: "anywhere"
            }
          },
          variants: {
            live: {
              true: {
                minHeight: "160px"
              }
            }
          }
        },
        desktopClientLogViewportRecipe: {
          className: "cs-desktop-client-log-viewport",
          description: "Desktop client live log scroll viewport.",
          base: {
            display: "grid",
            gap: "10px",
            maxHeight: "280px",
            overflow: "auto",
            paddingRight: "4px",
            scrollBehavior: "smooth"
          }
        },
        desktopClientLogStageRecipe: {
          className: "cs-desktop-client-log-stage",
          description: "Desktop client log stage block.",
          base: {
            display: "grid",
            gap: "7px",
            minWidth: 0,
            "& + &": {
              borderTop: "1px solid var(--border)",
              paddingTop: "10px"
            },
            "& span": {
              color: "var(--text-muted)",
              fontSize: "12px",
              fontWeight: "650",
              lineHeight: "1.35"
            }
          }
        },
        profileModeLayoutRecipe: {
          className: "cs-profile-mode-layout",
          description: "Profiles main page mode layout.",
          base: {
            display: "grid",
            gridTemplateColumns: "minmax(0, 1fr)",
            gap: "16px",
            alignItems: "start",
            "@media (max-width: 860px)": {
              gridTemplateColumns: "1fr"
            }
          }
        },
        profileModeSwitcherRecipe: {
          className: "cs-profile-mode-switcher",
          description: "Profiles configuration-file and gateway segmented control.",
          base: {
            display: "grid",
            gridTemplateColumns: "repeat(2, minmax(0, 1fr))",
            gap: "3px",
            width: "220px",
            maxWidth: "100%",
            minWidth: 0,
            padding: "3px",
            border: "1px solid var(--border)",
            borderRadius: "8px",
            background: "var(--surface-strong)",
            "& button": {
              display: "inline-flex",
              alignItems: "center",
              justifyContent: "center",
              minWidth: 0,
              minHeight: "30px",
              border: 0,
              borderRadius: "6px",
              background: "transparent",
              color: "var(--text-soft)",
              padding: "0 10px",
              fontSize: "11px",
              fontWeight: "800",
              whiteSpace: "nowrap",
              transition:
                "background var(--motion-quick), color var(--motion-quick), box-shadow var(--motion-quick)",
              _hover: {
                background: "var(--surface-hover)",
                color: "var(--text)"
              },
              _focusVisible: {
                outline: "2px solid color-mix(in srgb, var(--accent) 45%, transparent)",
                outlineOffset: "1px"
              }
            },
            "& button[data-selected='true']": {
              background: "var(--surface)",
              color: "var(--accent)",
              boxShadow: "0 1px 3px color-mix(in srgb, var(--modal-shadow) 20%, transparent)"
            },
            "@media (max-width: 860px)": {
              width: "100%"
            }
          }
        },
        profileToolSwitcherRecipe: {
          className: "cs-profile-tool-switcher",
          description: "Profiles tool switcher panel.",
          base: {
            position: "relative",
            zIndex: 2,
            minWidth: 0,
            overflow: "visible",
            border: "1px solid var(--border)",
            borderRadius: "var(--radius)",
            background: "var(--surface)",
            padding: "10px 8px 12px",
            transition:
              "border-color var(--motion-quick), background var(--motion-quick), box-shadow var(--motion-smooth), transform var(--motion-spring)"
          }
        },
        profileToolTabsRecipe: {
          className: "cs-profile-tool-tabs",
          description: "Profiles tool tab list.",
          base: {
            display: "flex",
            flexWrap: "wrap",
            gap: "8px",
            minWidth: 0,
            overflow: "visible",
            padding: 0,
            scrollSnapType: "x proximity",
            "& > button": {
              display: "grid",
              gridTemplateColumns: "auto minmax(0, 1fr)",
              alignItems: "center",
              gap: "8px",
              flex: "0 0 auto",
              minWidth: "136px",
              maxWidth: "196px",
              minHeight: "46px",
              border: "1px solid var(--border)",
              borderRadius: "7px",
              background: "var(--surface-strong)",
              color: "var(--text-soft)",
              padding: "7px 9px",
              textAlign: "left",
              scrollSnapAlign: "start",
              animation: "surface-rise 360ms cubic-bezier(0.16, 1, 0.3, 1) backwards",
              animationDelay: "calc(min(var(--surface-index, 0), 8) * 28ms)",
              transition:
                "background var(--motion-quick), border-color var(--motion-quick), color var(--motion-quick), box-shadow var(--motion-smooth), transform var(--motion-spring)",
              willChange: "transform, box-shadow",
              _hover: {
                borderColor: "var(--border-strong)",
                background: "var(--surface-hover)",
                color: "var(--text)",
                transform: "translateY(-2px) scale(1.012)",
                boxShadow: "0 10px 24px color-mix(in srgb, var(--modal-shadow) 26%, transparent)"
              }
            },
            "& > button:nth-child(1)": {
              "--surface-index": "1"
            },
            "& > button:nth-child(2)": {
              "--surface-index": "2"
            },
            "& > button:nth-child(3)": {
              "--surface-index": "3"
            },
            "& > button:nth-child(4)": {
              "--surface-index": "4"
            },
            "& > button:nth-child(5)": {
              "--surface-index": "5"
            },
            "& > button:nth-child(n + 6)": {
              "--surface-index": "6"
            },
            "& > button[data-selected='true']": {
              borderColor: "color-mix(in srgb, var(--accent) 45%, transparent)",
              background: "color-mix(in srgb, var(--accent) 10%, var(--surface-strong))",
              color: "var(--text)",
              boxShadow:
                "0 0 0 1px color-mix(in srgb, var(--accent) 10%, transparent), 0 10px 24px color-mix(in srgb, var(--modal-shadow) 18%, transparent)"
            },
            "& > button > span": {
              display: "grid",
              gap: "3px",
              minWidth: 0
            },
            "& strong, & small": {
              display: "block",
              minWidth: 0,
              overflow: "hidden",
              textOverflow: "ellipsis",
              whiteSpace: "nowrap"
            },
            "& strong": {
              color: "var(--text)",
              fontSize: "11px",
              fontWeight: "800",
              lineHeight: "1.2"
            },
            "& small": {
              color: "var(--text-muted)",
              fontSize: "11px",
              fontWeight: "700",
              lineHeight: "1.2"
            },
            "@media (max-width: 860px)": {
              "& > button": {
                minWidth: "124px"
              }
            },
            "@media (prefers-reduced-motion: reduce)": {
              "& > button": {
                animation: "none",
                transition: "none",
                transform: "none !important"
              },
              "& > button:hover": {
                transform: "none"
              }
            }
          }
        },
        profileToolSectionRecipe: {
          className: "cs-profile-tool-section",
          description: "Profiles selected tool profile list panel.",
          base: {
            minWidth: 0,
            overflow: "visible",
            border: "1px solid var(--border)",
            borderRadius: "var(--radius)",
            background: "var(--surface)",
            transition:
              "border-color var(--motion-quick), background var(--motion-quick), box-shadow var(--motion-smooth), transform var(--motion-spring)"
          }
        },
        profileGridRecipe: {
          className: "cs-profile-grid",
          description: "Profiles sortable card list.",
          base: {
            display: "flex",
            flexDirection: "column",
            gap: "12px",
            minWidth: 0,
            padding: "12px",
            "& > *:nth-child(1)": {
              "--surface-index": "1"
            },
            "& > *:nth-child(2)": {
              "--surface-index": "2"
            },
            "& > *:nth-child(3)": {
              "--surface-index": "3"
            },
            "& > *:nth-child(4)": {
              "--surface-index": "4"
            },
            "& > *:nth-child(5)": {
              "--surface-index": "5"
            },
            "& > *:nth-child(n + 6)": {
              "--surface-index": "6"
            }
          }
        },
        profileSortableRowRecipe: {
          className: "cs-profile-sortable-row",
          description: "Profiles sortable row wrapper.",
          base: {
            position: "relative",
            minWidth: 0,
            "&[data-sortable-active='true']": {
              zIndex: 8
            }
          }
        },
        profileCardRecipe: {
          className: "cs-profile-card",
          description: "Profiles compact profile card.",
          base: {
            position: "relative",
            display: "grid",
            gridTemplateColumns: "minmax(0, 1fr) auto auto",
            alignItems: "center",
            gap: "12px",
            minWidth: 0,
            border: "1px solid var(--border)",
            borderRadius: "var(--radius)",
            background: "var(--surface)",
            padding: "12px",
            userSelect: "none",
            animation: "surface-rise 360ms cubic-bezier(0.16, 1, 0.3, 1) backwards",
            animationDelay: "calc(min(var(--surface-index, 0), 8) * 28ms)",
            transition:
              "border-color var(--motion-quick), background var(--motion-quick), box-shadow var(--motion-smooth), opacity var(--motion-quick), transform 200ms cubic-bezier(0.2, 0.8, 0.2, 1)",
            willChange: "transform, box-shadow",
            _hover: {
              borderColor: "var(--border-strong)",
              boxShadow: "0 10px 24px color-mix(in srgb, var(--modal-shadow) 20%, transparent)"
            },
            "&[data-active='true']": {
              borderColor: "color-mix(in srgb, var(--accent) 42%, transparent)",
              background: "color-mix(in srgb, var(--accent) 7%, var(--surface))"
            },
            "&[data-builtin='true']": {
              borderColor: "var(--border)",
              background: "var(--surface)"
            },
            "&[data-drag-active='true']": {
              zIndex: 8,
              cursor: "grabbing",
              borderColor: "var(--accent)",
              transform: "scale(1.05)",
              boxShadow:
                "0 20px 42px color-mix(in srgb, var(--modal-shadow) 44%, transparent), 0 0 0 1px color-mix(in srgb, var(--accent) 24%, transparent)",
              transition:
                "border-color var(--motion-quick), background var(--motion-quick), box-shadow var(--motion-smooth)"
            },
            "&[data-drag-active='true'] [data-profile-drag-handle]": {
              color: "var(--accent)",
              cursor: "grabbing"
            },
            "&:hover [data-profile-avatar], &[data-drag-active='true'] [data-profile-avatar]": {
              transform: "scale(1.04)"
            },
            "@media (max-width: 860px)": {
              gridTemplateColumns: "minmax(0, 1fr)",
              alignItems: "stretch"
            },
            "@media (prefers-reduced-motion: reduce)": {
              animation: "none",
              transition: "none"
            }
          }
        },
        profileCardMainRecipe: {
          className: "cs-profile-card-main",
          description: "Profiles card identity row.",
          base: {
            display: "grid",
            gridTemplateColumns: "24px 34px minmax(0, 1fr)",
            alignItems: "center",
            gap: "10px",
            minWidth: 0
          }
        },
        profileDragHandleRecipe: {
          className: "cs-profile-drag-handle",
          description: "Profiles card drag handle.",
          base: {
            display: "grid",
            placeItems: "center",
            width: "24px",
            height: "32px",
            border: 0,
            borderRadius: "6px",
            background: "transparent",
            color: "var(--text-muted)",
            cursor: "grab",
            touchAction: "none",
            transition: "color var(--motion-quick), transform var(--motion-smooth)",
            _hover: {
              background: "transparent",
              color: "var(--accent)"
            },
            "&[aria-disabled='true']": {
              cursor: "default",
              opacity: 0.46
            }
          }
        },
        profileAvatarRecipe: {
          className: "cs-profile-avatar",
          description: "Profiles compact avatar and tool icon frame.",
          base: {
            display: "grid",
            placeItems: "center",
            width: "34px",
            height: "34px",
            overflow: "hidden",
            border: 0,
            borderRadius: "8px",
            background: "color-mix(in srgb, var(--accent) 10%, var(--surface-strong))",
            color: "var(--accent)",
            fontSize: "13px",
            fontWeight: "900",
            lineHeight: 1,
            textAlign: "center",
            transition:
              "background var(--motion-quick), color var(--motion-quick), transform var(--motion-smooth)",
            "& img": {
              width: "100%",
              height: "100%",
              objectFit: "cover"
            },
            "& [data-tool-icon-variant]": {
              display: "grid",
              flex: "0 0 100%",
              placeItems: "center",
              width: "100%",
              minWidth: "100%",
              height: "100%",
              minHeight: "100%",
              border: 0,
              borderRadius: "inherit"
            },
            "& [data-tool-icon-variant] img": {
              width: "78%",
              height: "78%",
              objectFit: "contain"
            },
            "&:has([data-tool-icon-tone='hermes'])": {
              overflow: "visible"
            },
            "& [data-tool-icon-tone='hermes'] img": {
              width: "36px",
              height: "36px",
              objectFit: "contain"
            }
          },
          variants: {
            size: {
              compact: {},
              large: {
                width: "56px",
                height: "56px",
                borderRadius: "10px",
                fontSize: "18px"
              }
            }
          },
          defaultVariants: {
            size: "compact"
          }
        },
        profileIdentityRecipe: {
          className: "cs-profile-identity",
          description: "Profiles card identity text.",
          base: {
            minWidth: 0,
            "& h2": {
              margin: 0,
              letterSpacing: 0,
              color: "var(--text)",
              fontSize: "14px",
              fontWeight: "700",
              lineHeight: "1.25",
              overflow: "hidden",
              textOverflow: "ellipsis",
              whiteSpace: "nowrap"
            },
            "& p": {
              margin: "5px 0 0",
              color: "var(--text-muted)",
              fontFamily: "ui-monospace, \"SFMono-Regular\", Consolas, monospace",
              fontSize: "12px",
              lineHeight: "1.35",
              overflow: "hidden",
              textOverflow: "ellipsis",
              whiteSpace: "nowrap"
            },
            "& [data-profile-remark]": {
              display: "-webkit-box",
              maxWidth: "100%",
              color: "var(--text-soft)",
              fontFamily: "inherit",
              whiteSpace: "normal",
              overflowWrap: "anywhere",
              WebkitLineClamp: 2,
              WebkitBoxOrient: "vertical"
            }
          }
        },
        profileCardStatusRecipe: {
          className: "cs-profile-card-status",
          description: "Profiles card status placement.",
          base: {
            display: "flex",
            justifyContent: "flex-end",
            "@media (max-width: 860px)": {
              justifySelf: "flex-start"
            }
          }
        },
        profileCardActionsRecipe: {
          className: "cs-profile-card-actions",
          description: "Profiles compact card action row.",
          base: {
            display: "flex",
            alignItems: "center",
            justifyContent: "flex-end",
            gap: "9px",
            flexWrap: "wrap",
            "@media (max-width: 860px)": {
              justifyContent: "flex-start"
            }
          }
        },
        profileDiffPanelRecipe: {
          className: "cs-profile-diff-panel",
          description: "Profile native diff and result panel.",
          base: {
            display: "grid",
            gap: "10px",
            minWidth: 0,
            border: "1px solid var(--border)",
            borderRadius: "var(--radius)",
            background: "var(--surface-strong)",
            padding: "12px"
          },
          variants: {
            tone: {
              neutral: {},
              accent: {
                borderColor: "color-mix(in srgb, var(--accent) 30%, transparent)",
                background: "color-mix(in srgb, var(--accent) 8%, var(--surface-strong))"
              },
              warning: {
                borderColor: "color-mix(in srgb, var(--amber) 34%, transparent)",
                background: "color-mix(in srgb, var(--amber) 10%, var(--surface-strong))"
              }
            }
          },
          defaultVariants: {
            tone: "neutral"
          }
        },
        profileDiffHeadingRecipe: {
          className: "cs-profile-diff-heading",
          description: "Profile diff panel heading row.",
          base: {
            display: "flex",
            alignItems: "flex-start",
            justifyContent: "space-between",
            gap: "12px",
            minWidth: 0,
            "& div": {
              minWidth: 0
            },
            "& strong, & span": {
              display: "block"
            },
            "& span": {
              marginTop: "4px",
              color: "var(--text-muted)",
              fontSize: "12px",
              overflowWrap: "anywhere"
            },
            "@media (max-width: 860px)": {
              display: "grid",
              justifyContent: "stretch"
            }
          }
        },
        profileDiffListRecipe: {
          className: "cs-profile-diff-list",
          description: "Profile native diff row list.",
          base: {
            display: "grid",
            gap: "8px"
          }
        },
        profileDiffRowRecipe: {
          className: "cs-profile-diff-row",
          description: "Profile native diff row.",
          base: {
            display: "grid",
            gridTemplateColumns: "minmax(72px, auto) minmax(0, 1.35fr) minmax(0, 1fr) minmax(0, 1fr)",
            gap: "8px",
            alignItems: "start",
            minWidth: 0,
            border: "1px solid var(--border)",
            borderRadius: "var(--radius)",
            background: "var(--surface-strong)",
            padding: "10px",
            "& > span": {
              display: "inline-flex",
              justifyContent: "center",
              minHeight: "22px",
              borderRadius: "999px",
              background: "color-mix(in srgb, var(--accent) 14%, transparent)",
              color: "var(--accent)",
              padding: "3px 8px",
              fontSize: "12px",
              fontWeight: "800",
              textTransform: "capitalize"
            },
            "& div": {
              display: "grid",
              gap: "4px",
              minWidth: 0
            },
            "& b": {
              color: "var(--text-muted)",
              fontFamily: "ui-monospace, \"SFMono-Regular\", Consolas, monospace",
              fontSize: "11px",
              textTransform: "uppercase"
            },
            "& em": {
              color: "var(--text-soft)",
              fontSize: "12px",
              fontStyle: "normal",
              lineHeight: "1.4",
              overflowWrap: "anywhere"
            },
            "& p": {
              gridColumn: "2 / -1",
              margin: 0,
              color: "var(--text-muted)",
              fontSize: "12px",
              lineHeight: "1.4"
            },
            "@media (max-width: 860px)": {
              gridTemplateColumns: "1fr",
              "& p": {
                gridColumn: "1"
              }
            }
          }
        },
        profileInlineNoticeRecipe: {
          className: "cs-profile-inline-notice",
          description: "Profile modal inline success/error notice.",
          base: {
            boxSizing: "border-box",
            width: "100%",
            borderRadius: "7px",
            padding: "10px 12px",
            fontSize: "13px",
            fontWeight: "800",
            overflowWrap: "anywhere"
          },
          variants: {
            tone: {
              error: {
                border: "1px solid color-mix(in srgb, var(--danger) 34%, transparent)",
                background: "color-mix(in srgb, var(--danger) 13%, transparent)",
                color: "var(--danger-text)"
              },
              success: {
                border: "1px solid color-mix(in srgb, var(--accent) 34%, transparent)",
                background: "color-mix(in srgb, var(--accent) 12%, transparent)",
                color: "var(--accent)"
              }
            }
          },
          defaultVariants: {
            tone: "error"
          }
        },
        profileUsageOfficialPanelRecipe: {
          className: "cs-profile-usage-official-panel",
          description: "Profile usage modal official OAuth notice.",
          base: {
            display: "grid",
            gridTemplateColumns: "auto minmax(0, 1fr)",
            alignItems: "center",
            gap: "10px",
            minWidth: 0,
            border: "1px solid color-mix(in srgb, var(--accent) 30%, transparent)",
            borderRadius: "7px",
            background: "color-mix(in srgb, var(--accent) 8%, var(--surface-strong))",
            color: "var(--text-soft)",
            padding: "12px",
            "& strong, & span": {
              display: "block",
              minWidth: 0,
              overflowWrap: "anywhere"
            },
            "& strong": {
              color: "var(--text)",
              fontSize: "13px"
            },
            "& span": {
              marginTop: "2px",
              fontSize: "12px"
            }
          }
        },
        profileEmbeddedStackRecipe: {
          className: "cs-profile-embedded-stack",
          description: "Embedded Profiles route stack.",
          base: {
            display: "grid",
            gap: "18px",
            width: "100%",
            minWidth: 0
          }
        },
        profileFormGridRecipe: {
          className: "cs-profile-form-grid",
          description: "Profile edit and usage form grid.",
          base: {
            display: "grid",
            gap: "12px",
            marginTop: "12px",
            maxWidth: "680px"
          },
          variants: {
            columns: {
              single: {},
              double: {
                maxWidth: "none",
                gridTemplateColumns: "repeat(2, minmax(0, 1fr))",
                "@media (max-width: 900px)": {
                  gridTemplateColumns: "1fr"
                }
              }
            }
          },
          defaultVariants: {
            columns: "single"
          }
        },
        profileFieldErrorRecipe: {
          className: "cs-profile-field-error",
          description: "Profile form field-level error text.",
          base: {
            display: "block",
            marginTop: "5px",
            color: "var(--danger-text)",
            fontSize: "12px",
            fontWeight: "700"
          }
        },
        profileIconEditorRecipe: {
          className: "cs-profile-icon-editor",
          description: "Profile edit modal icon editor.",
          base: {
            display: "grid",
            gridTemplateColumns: "56px minmax(0, 1fr)",
            alignItems: "end",
            gap: "12px",
            "@media (max-width: 900px)": {
              gridTemplateColumns: "1fr",
              alignItems: "start"
            }
          }
        },
        profileIconActionsRecipe: {
          className: "cs-profile-icon-actions",
          description: "Profile edit modal icon action row.",
          base: {
            gridColumn: "2",
            display: "flex",
            alignItems: "center",
            gap: "9px",
            flexWrap: "wrap",
            "& input[type='file']": {
              display: "none"
            },
            "@media (max-width: 900px)": {
              gridColumn: "auto"
            }
          }
        },
        profileUsageTemplateRowRecipe: {
          className: "cs-profile-usage-template-row",
          description: "Profile usage script template selector.",
          base: {
            display: "flex",
            gap: "8px",
            flexWrap: "wrap",
            "& button": {
              minHeight: "30px",
              border: "1px solid var(--border)",
              borderRadius: "7px",
              background: "var(--surface-strong)",
              color: "var(--text-soft)",
              padding: "0 10px",
              fontSize: "11px",
              fontWeight: "800",
              transition:
                "background var(--motion-quick), border-color var(--motion-quick), color var(--motion-quick)"
            },
            "& button:hover:not(:disabled)": {
              borderColor: "var(--border-strong)",
              background: "var(--surface-hover)",
              color: "var(--text)"
            },
            "& button[data-selected='true']": {
              borderColor: "color-mix(in srgb, var(--accent) 45%, transparent)",
              background: "color-mix(in srgb, var(--accent) 10%, var(--surface-strong))",
              color: "var(--accent)"
            }
          }
        },
        profileUsageCodeFieldRecipe: {
          className: "cs-profile-usage-code-field",
          description: "Profile usage script code editor field.",
          base: {
            gap: "8px",
            "& textarea": {
              minHeight: "260px",
              padding: "12px",
              resize: "vertical",
              fontFamily: "ui-monospace, \"SFMono-Regular\", Consolas, monospace",
              fontSize: "12px",
              lineHeight: "1.5"
            }
          }
        },
        profileUsageResultGridRecipe: {
          className: "cs-profile-usage-result-grid",
          description: "Profile usage query result card grid.",
          base: {
            display: "grid",
            gridTemplateColumns: "repeat(auto-fit, minmax(min(100%, 220px), 1fr))",
            gap: "10px"
          }
        },
        profileUsageResultCardRecipe: {
          className: "cs-profile-usage-result-card",
          description: "Profile usage query result card.",
          base: {
            display: "grid",
            gap: "8px",
            minWidth: 0,
            border: "1px solid var(--border)",
            borderRadius: "7px",
            background: "var(--surface)",
            padding: "12px",
            "&[data-invalid='true']": {
              borderColor: "color-mix(in srgb, var(--danger) 40%, transparent)"
            },
            "& strong, & span, & small": {
              minWidth: 0,
              overflowWrap: "anywhere"
            },
            "& dl": {
              display: "grid",
              gridTemplateColumns: "repeat(3, minmax(0, 1fr))",
              gap: "8px",
              margin: 0
            },
            "& dt": {
              color: "var(--text-muted)",
              fontSize: "11px",
              fontWeight: "800"
            },
            "& dd": {
              margin: "3px 0 0",
              color: "var(--text)",
              fontSize: "13px",
              fontWeight: "800"
            },
            "& [data-usage-balance]": {
              color: "var(--accent)",
              fontSize: "15px",
              fontWeight: "900"
            },
            "&[data-invalid='true'] [data-usage-balance]": {
              color: "var(--danger-text)"
            }
          }
        },
        profileWriteContentPreviewRecipe: {
          className: "cs-profile-write-content-preview",
          description: "Profile native write content preview.",
          base: {
            display: "grid",
            gap: "8px",
            minWidth: 0,
            border: "1px solid var(--border)",
            borderRadius: "7px",
            background: "var(--surface)",
            padding: "10px",
            "& strong": {
              color: "var(--text)",
              fontSize: "12px"
            },
            "& pre": {
              maxHeight: "260px",
              margin: 0,
              overflow: "auto",
              border: "1px solid var(--border)",
              borderRadius: "6px",
              background: "var(--code-bg)",
              color: "var(--code-text)",
              padding: "10px",
              fontFamily: "ui-monospace, \"SFMono-Regular\", Consolas, monospace",
              fontSize: "12px",
              lineHeight: "1.45",
              whiteSpace: "pre-wrap",
              overflowWrap: "anywhere"
            }
          }
        },
        wizardActionsRecipe: {
          className: "cs-wizard-actions",
          description: "Setup wizard top action row.",
          base: {
            display: "flex",
            alignItems: "center",
            justifyContent: "flex-end",
            gap: "9px",
            flexWrap: "wrap",
            "@media (max-width: 860px)": {
              justifyContent: "flex-start",
              "& > button": {
                flex: "1 1 150px"
              }
            }
          }
        },
        wizardStepperRecipe: {
          className: "cs-wizard-stepper",
          description: "Setup wizard compact stepper.",
          base: {
            display: "grid",
            gridTemplateColumns: "repeat(7, minmax(42px, 1fr))",
            gap: "8px"
          }
        },
        wizardStepItemRecipe: {
          className: "cs-wizard-step-item",
          description: "Setup wizard stepper item.",
          base: {
            display: "grid",
            placeItems: "center",
            minHeight: "34px",
            border: "1px solid var(--border)",
            borderRadius: "7px",
            background: "var(--surface-soft)",
            color: "var(--text-muted)",
            "&[data-step-state='active']": {
              borderColor: "color-mix(in srgb, var(--accent) 42%, transparent)",
              background: "var(--accent)",
              color: "var(--accent-ink)"
            },
            "&[data-step-state='done']": {
              borderColor: "color-mix(in srgb, var(--accent) 20%, transparent)",
              background: "color-mix(in srgb, var(--accent) 16%, transparent)",
              color: "var(--accent)"
            }
          }
        },
        wizardPanelRecipe: {
          className: "cs-wizard-panel",
          description: "Setup wizard main panel.",
          base: {
            minWidth: 0,
            overflow: "visible",
            border: "1px solid var(--border)",
            borderRadius: "var(--radius)",
            background: "var(--surface)",
            padding: "18px",
            transition:
              "border-color var(--motion-quick), background var(--motion-quick), box-shadow var(--motion-smooth), transform var(--motion-spring)"
          }
        },
        wizardStepContentRecipe: {
          className: "cs-wizard-step-content",
          description: "Setup wizard animated step content.",
          base: {
            display: "grid",
            gap: "12px",
            minWidth: 0
          }
        },
        wizardChoiceGridRecipe: {
          className: "cs-wizard-choice-grid",
          description: "Setup wizard choice button grid.",
          base: {
            display: "grid",
            gap: "12px"
          },
          variants: {
            kind: {
              tool: {
                gridTemplateColumns: "repeat(auto-fit, minmax(156px, 1fr))",
                "& > button:nth-child(1)": {
                  "--surface-index": "1"
                },
                "& > button:nth-child(2)": {
                  "--surface-index": "2"
                },
                "& > button:nth-child(3)": {
                  "--surface-index": "3"
                },
                "& > button:nth-child(4)": {
                  "--surface-index": "4"
                },
                "& > button:nth-child(5)": {
                  "--surface-index": "5"
                },
                "& > button:nth-child(n + 6)": {
                  "--surface-index": "6"
                }
              },
              compact: {
                marginTop: "12px",
                maxWidth: "680px",
                gridTemplateColumns: "repeat(auto-fit, minmax(210px, 1fr))"
              }
            }
          },
          defaultVariants: {
            kind: "compact"
          }
        },
        wizardChoiceButtonRecipe: {
          className: "cs-wizard-choice-button",
          description: "Setup wizard selectable choice button.",
          base: {
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            flexWrap: "wrap",
            gap: "8px",
            minHeight: "44px",
            border: "1px solid var(--border)",
            borderRadius: "7px",
            background: "var(--surface-strong)",
            color: "var(--text-soft)",
            fontSize: "11px",
            fontWeight: "700",
            transition:
              "background var(--motion-quick), border-color var(--motion-quick), color var(--motion-quick), opacity var(--motion-quick), transform var(--motion-spring), box-shadow var(--motion-smooth)",
            willChange: "transform, box-shadow",
            _hover: {
              borderColor: "var(--border-strong)",
              background: "var(--surface-hover)",
              color: "var(--text)",
              transform: "translateY(-3px) scale(1.008)",
              boxShadow: "0 14px 30px color-mix(in srgb, var(--modal-shadow) 24%, transparent)"
            },
            _disabled: {
              cursor: "not-allowed",
              opacity: 0.54
            },
            "& small": {
              width: "100%",
              color: "var(--text-muted)",
              fontSize: "10.5px",
              fontWeight: "700",
              textAlign: "center"
            },
            "&[data-selected='true']": {
              borderColor: "color-mix(in srgb, var(--accent) 44%, transparent)",
              background: "color-mix(in srgb, var(--accent) 12%, var(--surface-strong))",
              color: "var(--accent)",
              boxShadow: "0 0 0 1px color-mix(in srgb, var(--accent) 10%, transparent)"
            },
            "@media (prefers-reduced-motion: reduce)": {
              animation: "none",
              transition: "none",
              transform: "none !important",
              _hover: {
                transform: "none"
              }
            }
          },
          variants: {
            kind: {
              tool: {
                display: "grid",
                gridTemplateRows: "auto auto auto",
                placeItems: "center",
                alignContent: "center",
                gap: "6px",
                minHeight: "78px",
                padding: "12px 10px 10px",
                textAlign: "center",
                animation: "surface-rise 360ms cubic-bezier(0.16, 1, 0.3, 1) backwards",
                animationDelay: "calc(min(var(--surface-index, 0), 8) * 28ms)",
                "& [data-tool-icon-variant='choice']": {
                  width: "35px",
                  minWidth: "35px",
                  height: "35px",
                  minHeight: "35px"
                },
                "& [data-tool-icon-variant='choice'] img": {
                  width: "23px",
                  height: "23px"
                },
                "& > span, & small": {
                  minWidth: 0,
                  maxWidth: "100%",
                  overflow: "hidden",
                  textOverflow: "ellipsis",
                  whiteSpace: "nowrap"
                }
              },
              compact: {}
            }
          },
          defaultVariants: {
            kind: "compact"
          }
        },
        wizardModeChoiceRecipe: {
          className: "cs-wizard-mode-choice",
          description: "Setup wizard provider mode section.",
          base: {
            display: "grid",
            gap: "10px",
            marginTop: "16px",
            color: "var(--text-soft)",
            fontSize: "13px",
            fontWeight: "800"
          }
        },
        wizardFormGridRecipe: {
          className: "cs-wizard-form-grid",
          description: "Setup wizard connection form grid.",
          base: {
            display: "grid",
            gap: "12px",
            marginTop: "12px",
            maxWidth: "680px"
          }
        },
        wizardWideFieldRecipe: {
          className: "cs-wizard-wide-field",
          description: "Setup wizard form field that spans the full grid width.",
          base: {
            gridColumn: "1 / -1"
          }
        },
        wizardFieldErrorRecipe: {
          className: "cs-wizard-field-error",
          description: "Setup wizard field-level error text.",
          base: {
            display: "block",
            marginTop: "5px",
            color: "var(--danger-text)",
            fontSize: "12px",
            fontWeight: "700"
          }
        },
        wizardInlineNoticeRecipe: {
          className: "cs-wizard-inline-notice",
          description: "Setup wizard inline status notice.",
          base: {
            boxSizing: "border-box",
            width: "100%",
            borderRadius: "7px",
            padding: "10px 12px",
            fontSize: "13px",
            fontWeight: "800",
            overflowWrap: "anywhere"
          },
          variants: {
            tone: {
              error: {
                border: "1px solid color-mix(in srgb, var(--danger) 34%, transparent)",
                background: "color-mix(in srgb, var(--danger) 13%, transparent)",
                color: "var(--danger-text)"
              },
              success: {
                border: "1px solid color-mix(in srgb, var(--accent) 34%, transparent)",
                background: "color-mix(in srgb, var(--accent) 12%, transparent)",
                color: "var(--accent)"
              }
            }
          },
          defaultVariants: {
            tone: "error"
          }
        },
        wizardCodexAuthCardRecipe: {
          className: "cs-wizard-codex-auth-card",
          description: "Setup wizard Codex OAuth authorization card.",
          base: {
            display: "grid",
            gridTemplateColumns: "minmax(0, 1fr) auto",
            alignItems: "center",
            gap: "12px",
            width: "min(100%, 820px)",
            marginTop: "14px",
            border: "1px solid var(--border)",
            borderRadius: "var(--radius)",
            background: "var(--surface-strong)",
            padding: "13px",
            "& > div:first-child": {
              display: "grid",
              gap: "5px",
              minWidth: 0
            },
            "& strong": {
              color: "var(--text)",
              fontSize: "13px"
            },
            "& span": {
              color: "var(--text-soft)",
              fontSize: "13px",
              lineHeight: "1.45"
            },
            "& small": {
              color: "var(--text-muted)",
              fontSize: "12px",
              overflowWrap: "anywhere"
            },
            "@media (max-width: 860px)": {
              gridTemplateColumns: "1fr"
            }
          }
        },
        wizardButtonRowRecipe: {
          className: "cs-wizard-button-row",
          description: "Setup wizard inline button row.",
          base: {
            display: "flex",
            alignItems: "center",
            justifyContent: "flex-end",
            gap: "9px",
            flexWrap: "wrap"
          }
        },
        wizardSecurityNoteRecipe: {
          className: "cs-wizard-security-note",
          description: "Setup wizard API key security note.",
          base: {
            display: "flex",
            alignItems: "center",
            gap: "10px",
            marginTop: "14px",
            border: "1px solid var(--border)",
            borderRadius: "var(--radius)",
            background: "var(--surface-strong)",
            color: "var(--text-soft)",
            padding: "12px"
          }
        },
        wizardPreviewBoxRecipe: {
          className: "cs-wizard-preview-box",
          description: "Setup wizard write preview container.",
          base: {
            display: "grid",
            gap: "14px"
          }
        },
        wizardPreviewHeadingRecipe: {
          className: "cs-wizard-preview-heading",
          description: "Setup wizard section heading.",
          base: {
            display: "flex",
            alignItems: "flex-start",
            justifyContent: "space-between",
            gap: "12px",
            "@media (max-width: 860px)": {
              display: "grid",
              justifyContent: "stretch"
            }
          }
        },
        wizardWritePreviewListRecipe: {
          className: "cs-wizard-write-preview-list",
          description: "Setup wizard write preview list.",
          base: {
            display: "grid",
            gap: "10px",
            maxWidth: "820px"
          }
        },
        wizardWritePreviewRowRecipe: {
          className: "cs-wizard-write-preview-row",
          description: "Setup wizard write preview row.",
          base: {
            display: "grid",
            gap: "5px",
            border: "1px solid var(--border)",
            borderRadius: "var(--radius)",
            background: "var(--surface-strong)",
            padding: "10px 12px",
            "& strong": {
              color: "var(--text)",
              fontSize: "13px"
            },
            "& span": {
              color: "var(--text-muted)",
              fontSize: "13px",
              lineHeight: "1.4",
              overflowWrap: "anywhere"
            },
            "& code": {
              width: "100%",
              border: "1px solid var(--border)",
              borderRadius: "6px",
              background: "var(--code-bg)",
              color: "var(--code-text)",
              padding: "6px 8px",
              fontFamily: "ui-monospace, \"SFMono-Regular\", Consolas, monospace",
              fontSize: "12px",
              lineHeight: "1.45",
              overflowWrap: "anywhere",
              whiteSpace: "normal"
            }
          }
        },
        wizardWritePreviewMetaRecipe: {
          className: "cs-wizard-write-preview-meta",
          description: "Setup wizard write preview metadata row.",
          base: {
            display: "flex",
            alignItems: "center",
            gap: "9px",
            flexWrap: "wrap",
            "& b": {
              color: "var(--accent)",
              fontFamily: "ui-monospace, \"SFMono-Regular\", Consolas, monospace",
              fontSize: "11px",
              textTransform: "uppercase"
            }
          }
        },
        wizardWriteContentPreviewRecipe: {
          className: "cs-wizard-write-content-preview",
          description: "Setup wizard generated content preview block.",
          base: {
            display: "grid",
            gap: "8px",
            minWidth: 0,
            border: "1px solid var(--border)",
            borderRadius: "7px",
            background: "var(--surface)",
            padding: "10px",
            "& strong": {
              color: "var(--text)",
              fontSize: "12px"
            },
            "& pre": {
              maxHeight: "260px",
              margin: 0,
              overflow: "auto",
              border: "1px solid var(--border)",
              borderRadius: "6px",
              background: "var(--code-bg)",
              color: "var(--code-text)",
              padding: "10px",
              fontFamily: "ui-monospace, \"SFMono-Regular\", Consolas, monospace",
              fontSize: "12px",
              lineHeight: "1.45",
              whiteSpace: "pre-wrap",
              overflowWrap: "anywhere"
            }
          }
        },
        wizardPreviewWarningsRecipe: {
          className: "cs-wizard-preview-warnings",
          description: "Setup wizard preview warning chips.",
          base: {
            display: "flex",
            gap: "8px",
            flexWrap: "wrap",
            "& span": {
              border: "1px solid color-mix(in srgb, var(--amber) 28%, transparent)",
              borderRadius: "999px",
              background: "color-mix(in srgb, var(--amber) 12%, transparent)",
              color: "var(--warn-text)",
              padding: "4px 8px",
              fontSize: "12px",
              fontWeight: "800"
            }
          }
        },
        nativeToggleRecipe: {
          className: "cs-native-toggle",
          description: "Checkbox setting row used by desktop client pages.",
          base: {
            display: "grid",
            gridTemplateColumns: "auto minmax(0, 1fr)",
            alignItems: "center",
            gap: "10px",
            minWidth: 0,
            border: "1px solid var(--border)",
            borderRadius: "7px",
            background: "var(--surface-strong)",
            color: "var(--text-soft)",
            padding: "10px",
            fontSize: "13px",
            "& input[type='checkbox']": {
              width: "18px",
              height: "18px",
              minHeight: "18px",
              padding: 0,
              accentColor: "var(--accent)"
            },
            "& span, & strong, & small": {
              display: "block"
            },
            "& strong": {
              color: "var(--text)",
              fontSize: "13px"
            },
            "& small": {
              marginTop: "2px",
              color: "var(--text-muted)",
              fontSize: "12px",
              lineHeight: "1.35"
            }
          }
        },
        doctorListRecipe: {
          className: "cs-doctor-list",
          description: "Diagnostic capability list.",
          base: {
            display: "grid",
            gap: "var(--space-md)",
            padding: "var(--space-lg)"
          }
        },
        doctorRowRecipe: {
          className: "cs-doctor-row",
          description: "Diagnostic capability row.",
          base: {
            display: "grid",
            gridTemplateColumns: "auto minmax(0, 1fr)",
            alignItems: "center",
            gap: "12px",
            minWidth: 0,
            border: "1px solid var(--border)",
            borderRadius: "var(--radius)",
            background: "var(--surface-strong)",
            padding: "12px",
            "& h3": {
              margin: 0,
              color: "var(--text)",
              fontSize: "14px",
              lineHeight: "1.25"
            },
            "& p": {
              margin: "6px 0 0",
              color: "var(--text-muted)",
              overflowWrap: "anywhere",
              fontSize: "13px",
              lineHeight: "1.45"
            }
          }
        },
        toolCardRecipe: {
          className: "cs-tool-card",
          description: "Tool status card layout.",
          base: {
            display: "grid",
            gridTemplateColumns: "minmax(0, 1fr) auto",
            gridTemplateRows: "minmax(0, 1fr) auto",
            alignItems: "stretch",
            gap: "12px",
            minHeight: "78px",
            padding: "14px",
            border: "1px solid var(--border)",
            borderRadius: "var(--radius)",
            background: "var(--surface-strong)",
            transition:
              "border-color var(--motion-quick), background var(--motion-quick), box-shadow var(--motion-smooth), transform var(--motion-spring)",
            willChange: "transform, box-shadow",
            _hover: {
              borderColor: "var(--border-strong)",
              background: "var(--tool-hover-bg)",
              transform: "translateY(-2px)",
              boxShadow: "0 10px 24px color-mix(in srgb, var(--modal-shadow) 26%, transparent)"
            },
            "@media (prefers-reduced-motion: reduce)": {
              animation: "none",
              transition: "none",
              transform: "none !important",
              _hover: {
                transform: "none"
              }
            },
            "@media (max-width: 860px)": {
              gridTemplateColumns: "1fr"
            }
          }
        },
        toolMainRecipe: {
          className: "cs-tool-main",
          description: "Leading icon and copy cluster for a tool card.",
          base: {
            display: "flex",
            alignItems: "flex-start",
            gridColumn: "1 / -1",
            gridRow: "1",
            minWidth: 0,
            gap: "12px",
            "& h3": {
              margin: 0,
              color: "var(--text)",
              fontSize: "14px",
              lineHeight: "1.25"
            },
            "& p": {
              margin: "6px 0 0",
              color: "var(--text-muted)",
              overflowWrap: "anywhere",
              fontSize: "13px",
              lineHeight: "1.45"
            }
          }
        },
        toolStateRecipe: {
          className: "cs-tool-state",
          description: "Status pill placement for a tool card.",
          base: {
            display: "flex",
            gap: "7px",
            gridColumn: "1",
            gridRow: "2",
            flexWrap: "wrap",
            justifyContent: "flex-start",
            alignSelf: "end"
          }
        },
        toolActionRecipe: {
          className: "cs-tool-action",
          description: "Action button group for a tool card.",
          base: {
            display: "flex",
            alignSelf: "end",
            alignItems: "center",
            justifyContent: "flex-end",
            flexFlow: "row wrap",
            gap: "8px",
            gridColumn: "2",
            gridRow: "2",
            "@media (max-width: 860px)": {
              justifyContent: "flex-start"
            }
          }
        }
      }
    }
  }
});
