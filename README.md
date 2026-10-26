# Skylands Untrapped

[![AI Ready](https://img.shields.io/badge/AI--Ready-yes-brightgreen?style=flat)](https://github.com/johnpapa/ai-ready)

Skylanders Trap Team (Wii version) PC reimplementation (AI assisted)

## Objectives
- [ ] Support EU (en) version of the game
- [ ] Support Linux and Windows
- [ ] Support hardware and software portal
- [ ] Reach a playable state from start to finish
- [ ] Modern controls and graphics
- [ ] Higher refresh rate (144hz)
- [ ] Original game compatible save files

## Disclaimer

This project is an unofficial, fan-made, non-commercial decompilation of
*Skylanders: Trap Team*. It is not affiliated with, endorsed by, sponsored by,
or approved by Activision Publishing, Inc., Toys for Bob, Vicarious Visions,
or any of their parent companies, subsidiaries, or licensors.

*Skylanders*, *Skylanders: Trap Team*, and all related names, characters,
logos, and trademarks are the property of their respective owners. All
rights are reserved to those owners.

### Assets

**This repository does not contain, and will never distribute, any
copyrighted game assets.** This includes, but is not limited to, models,
textures, animations, audio, music, voice lines, videos, level data,
text/localization data, and any other data extracted from the original game.

- The repository contains only original source code produced through
  reverse engineering, plus build tooling.
- To build or run the project, you must supply your own legally obtained copy
  of the game. The tooling may extract required assets from your own copy
  locally; those extracted files must not be redistributed.
- Do not submit pull requests, issues, or any other contributions that
  include copyrighted assets, game binaries, keys, or extracted data.
  Such contributions will be removed.

### Purpose and Legal

This project is intended solely for educational, research, preservation, and
interoperability purposes. It does not condone or support piracy, and it does
not include any means of circumventing licensing of the original game.

The software is provided "as is", without warranty of any kind. The authors
are not responsible for any misuse of this project or for any damages arising
from its use. If you are a rights holder and believe that any content in this
repository infringes your rights, please open an issue or contact the
maintainers, and it will be promptly reviewed and, if appropriate, removed.

## Contributing

Contributions are welcome! Please make sure:
1. No copyrighted game assets or ROM files are submitted.
2. Code passes formatting and clippy lints (`devenv shell -- cargo fmt --all && devenv shell -- cargo clippy --all-targets --all-features`).
3. Changes include tests verifying new behavior.
4. Review [`AGENTS.md`](./AGENTS.md) for full development workflow and architecture details.
