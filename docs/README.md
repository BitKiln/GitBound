# GitBound documentation

Start with the [user guide](USER_GUIDE.md) if you are setting GitBound up for
the first time. The rest of this folder is organised by what you are trying to
do.

## Using GitBound

| Document                                     | What it covers                                                                                                                   |
| -------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------- |
| [User guide](USER_GUIDE.md)                  | Profile setup, SSH authentication, commit signing, repository binding, the desktop views, the command line, and troubleshooting. |
| [Continuous integration](CI.md)              | `gitbound verify` and `gitbound audit`, the GitHub Action, report formats, and the optional `.gitbound.toml` policy file.        |
| [Example workflow](examples/gitbound-ci.yml) | A complete GitHub Actions workflow using the action.                                                                             |

## Building and contributing

| Document                                                | What it covers                                                                                                       |
| ------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------- |
| [Building the desktop application](BUILDING_DESKTOP.md) | Toolchain requirements, everyday commands, release binaries, installers, and the four rules the frontend depends on. |
| [Design system](DESIGN_SYSTEM.md)                       | The visual and interaction language of the desktop app: tokens, layout, components, and the rules behind them.       |
| [Release QA checklist](QA_CHECKLIST.md)                 | The manual passes run against a build before it is released.                                                         |

## Elsewhere in the repository

- [README](../README.md) — what GitBound is, installation, and a quick start.
- [ARCHITECTURE](../ARCHITECTURE.md) — the crate layout, what each module
  decides, and the two invariants to know before changing anything.
- [CONTRIBUTING](../CONTRIBUTING.md) — branch model, review expectations, and the
  frontend/backend contract.
- [SECURITY](../SECURITY.md) — threat model, supported versions, how to report a
  vulnerability, and how to verify a release download.
- [CODE_OF_CONDUCT](../CODE_OF_CONDUCT.md) — the standards expected of everyone
  taking part.
- [CHANGELOG](../CHANGELOG.md) — what changed in each release.
- [PRODUCT](../PRODUCT.md) — the product definition the implementation is held
  to: audience, purpose, and the constraints that do not bend.
