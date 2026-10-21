# Tripos

A Blade Runner themed SET card game, written in Rust and compiled to
WebAssembly. Static site on GitHub Pages, built for touch: a phone for solo
play, an iPad flat on the table for a group.

## Play

Every card has four attributes with three values each: number (one, two,
three), shape (diamond, pill, squiggle), colour, and shading (solid, striped,
open). Three cards are a set when each attribute is all the same or all
different across the three. Tap three cards to claim a set.

Three modes, switched with the control under the table:

- **Solo**: race the clock through the whole deck. A wrong pick adds 10
  seconds, `HINT` reveals one card of a set for 15 seconds (press again for a
  second card), and `DEAL 3` adds cards when no set is out (15 seconds if a
  set was there after all). The best time is kept.
- **Party**: two to four players around a tablet. Each player has a bar on
  their edge of the screen; bars on the far side are flipped so people across
  the table can read them. Tap your bar to call a set, then pick three cards
  within seven seconds. Right is +1, wrong or late is -1. The deck runs out
  and the high score wins.
- **Daily**: one deck order per calendar day, the same for everyone, seeded
  from the date. Your best time for the day and your streak of finished days
  are kept, and `SHARE` copies a one-line result.

Three themes, switched with the second control under the table (`T` cycles
them):

- **Neon**: the Blade Runner look, pink, cyan and violet on near-black.
- **Classic**: a clean card-table look, red, green and purple on white.
- **Night**: an ultra-dark mode for bedtime. Pure black, dim warm tones, no
  glow or flicker.

The choice is remembered and applied before the first paint. Games in
progress are saved too.

Keys for desktop play: letters on the `qwerty` rows pick cards, `H` hint,
`D` deal, `N` new game, `R` rules, `1` to `4` call a set in Party, `Esc`
clears the selection.

## Layout

- `engine/`: the rules (`is_set`, `third`, `find_sets`), the deck and table
  (`Game`, twelve cards topped up after each set, three more only on request),
  and daily seeding as a pure Rust crate with no dependencies
  (`cargo test -p tripos-engine`).
- `src/`: the browser game: state, persistence, SVG card faces, rendering and
  input through `web-sys`, compiled to WebAssembly with `wasm-bindgen`.
- `web/`: the static page, stylesheet and icon.
- `build.sh`: builds the module and assembles `dist/`.
- `.github/workflows/pages.yml`: tests, builds and deploys `dist/` to GitHub
  Pages on every push to `main`.

## Build

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version "$(cargo metadata --format-version 1 | python3 -c 'import json,sys; print(next(p["version"] for p in json.load(sys.stdin)["packages"] if p["name"]=="wasm-bindgen"))')"
cargo test --workspace
./build.sh            # or ./build.sh debug
```

Then serve `dist/` with any static file server, for example:

```sh
cd dist && python3 -m http.server 8000
```

The server must send `.wasm` as `application/wasm` for streaming compilation;
GitHub Pages and Python's server both do.

## Hosting on GitHub Pages

1. In the repository settings open **Pages**.
2. Under **Build and deployment**, set **Source** to **GitHub Actions**.
3. Push to `main`. The workflow publishes the site at
   `https://<user>.github.io/tripos/`.
