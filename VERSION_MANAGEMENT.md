# Version Management

This project uses a centralized version management system for local development and CI/CD.

## Local Development

### Setup
1. Copy `.env.example` to `.env`:
   ```bash
   cp .env.example .env
   ```

2. Edit `.env` and set your desired version:
   ```bash
   VERSION=v0.2.8
   ```

### How it Works
- **`src/run.sh`**: Reads version from `.env` to construct the deploy binary download URL
- **`src/run-node.sh`**: Reads version from `.env` to construct the webserver binary download URL
- **`src/deploy/src/main.rs`**: Reads version from `.env` to download `run-node.sh`
- **`README.md`**: Contains `__VERSION__` placeholders that get replaced during build

The scripts will:
1. First check if `GITHUB_TARBALL_URL` environment variable is set
2. If not, attempt to load version from `.env` file
3. Fall back to hardcoded default if `.env` is not found

## CI/CD (GitHub Actions)

When a new tag is pushed (e.g., `v0.2.9`), the GitHub Actions workflow:

1. **Checks out the repository**
2. **Injects the version** from the git tag into:
   - `.env` file
   - `src/run.sh` (replaces download URLs)
   - `src/run-node.sh` (replaces download URLs)
   - `src/deploy/src/main.rs` (replaces hardcoded version)
   - `README.md` (replaces `__VERSION__` placeholders)
3. **Builds the binaries** with the injected version
4. **Creates a GitHub release** with the updated scripts

### Workflow Details
See `.github/workflows/release.yml` for the complete workflow. The key step is:

```yaml
- name: Inject version into scripts and README
  shell: bash
  run: |
    TAG="${GITHUB_REF_NAME}"
    echo "VERSION=$TAG" > .env
    sed -i "s|releases/download/v[0-9]\+\.[0-9]\+\.[0-9]\+/|releases/download/$TAG/|g" src/run.sh
    sed -i "s|releases/download/v[0-9]\+\.[0-9]\+\.[0-9]\+/|releases/download/$TAG/|g" src/run-node.sh
    sed -i "s|\"v[0-9]\+\.[0-9]\+\.[0-9]\+\"|\"$TAG\"|g" src/deploy/src/main.rs
    sed -i "s|__VERSION__|$TAG|g" README.md
```

## Creating a New Release

1. Update the version in your local `.env` if needed for testing
2. Commit your changes
3. Create and push a new tag:
   ```bash
   git tag v0.2.9
   git push origin v0.2.9
   ```
4. GitHub Actions will automatically:
   - Inject the version into all files
   - Build the binaries
   - Create a release with the updated scripts and binaries
   - The released `run.sh`, `run-node.sh`, and `README.md` will all reference `v0.2.9`

## Files Involved

- **`.env`**: Local version configuration (gitignored)
- **`.env.example`**: Template for `.env`
- **`src/run.sh`**: Deploy script (reads from `.env` locally, version injected in CI)
- **`src/run-node.sh`**: Node startup script (reads from `.env` locally, version injected in CI)
- **`src/deploy/src/main.rs`**: Rust deploy binary (reads from `.env` locally, version injected in CI)
- **`README.md`**: Documentation (uses `__VERSION__` placeholders, replaced in CI)
- **`.github/workflows/release.yml`**: GitHub Actions workflow that performs version injection

## Benefits

✅ Single source of truth for versions  
✅ No manual find-and-replace across multiple files  
✅ Local development uses `.env` for easy testing  
✅ CI/CD automatically updates all version references  
✅ README always shows the correct version in releases  
✅ Reduced chance of version mismatches
