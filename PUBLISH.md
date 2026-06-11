# Publishing fountain_transfer (single GitHub repo)

One GitHub repository contains **both** `fountain_transfer_core` and `fountain_transfer`.  
Published Raptor scheme crates (`fountain_raptor_q`, `fountain_raptor_10`) are **not** modified.

---

## One-time setup

### 1. Create the GitHub repo

Create **`fountain_transfer`** on GitHub (e.g. `https://github.com/shhyang/fountain_transfer`).  
Do not initialize with a README if you will push a full copy.

### 2. Clone into the monorepo (optional local mirror)

```bash
mkdir -p publish
git clone https://github.com/shhyang/fountain_transfer.git publish/fountain_transfer
```

`publish/` is gitignored in the monorepo.

---

## Sync from monorepo

From the **monorepo root** (`erasure_coding_lib_office/`):

```bash
./standalone/sync-from-monorepo.sh publish/fountain_transfer
```

Or default target `publish/fountain_transfer`:

```bash
./standalone/sync-from-monorepo.sh
```

The script:

1. Copies `fountain_transfer_core/` and `fountain_transfer/` sources (no monorepo `Cargo.toml`)
2. Installs workspace root `standalone/Cargo.toml`
3. Installs `Cargo.standalone.toml` → each crate's `Cargo.toml`
4. Copies `standalone/README.md` to the repo root

---

## Pre-release checklist

In the publish clone:

```bash
cd publish/fountain_transfer
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build -p fountain_transfer --release
```

Optional crates.io dry-run (publish **core** first):

```bash
cargo publish -p fountain_transfer_core --dry-run
cargo publish -p fountain_transfer --dry-run
```

---

## Push to GitHub

```bash
cd publish/fountain_transfer
git add .
git status
git commit -m "Release fountain_transfer workspace v1.0.0"
git tag v1.0.0
git push -u origin main
git push origin v1.0.0
```

---

## crates.io (optional, same repo)

From the publish clone root:

```bash
cargo publish -p fountain_transfer_core
cargo publish -p fountain_transfer
```

Both crates use `repository = "https://github.com/shhyang/fountain_transfer"`.

---

## Layout after sync

```
fountain_transfer/          # GitHub repo root
├── Cargo.toml            # workspace
├── README.md
├── fountain_transfer_core/
│   ├── Cargo.toml        # registry + git deps
│   └── src/ ...
└── fountain_transfer/
    ├── Cargo.toml        # path dep on ../fountain_transfer_core
    └── src/ ...
```
