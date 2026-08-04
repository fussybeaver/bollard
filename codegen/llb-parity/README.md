# BuildKit LLB parity oracle

This pinned Go module generates the BuildKit LLB definitions consumed by the
`bollard-llb` compatibility tests. The generated definitions remain owned by
the crate under `../../llb/testdata/golden/`.

## Regenerate

From this directory, run:

```bash
./regenerate.sh
```

The script generates into a temporary directory, checks the generated
definitions against the committed goldens and manifest, and only then copies
the results into `llb/testdata/golden/`. The generator binary is not checked
in.

To generate into another directory without updating the committed fixtures:

```bash
go run . -out /tmp/llb-golden
```

The BuildKit module version is pinned in `go.mod`. The manifest records the
generator version, module version, fixture platforms, and fixture hashes.
Increment the generator version when a generator change intentionally alters
the fixture format or output contract.

## Independent solve

`independent_solve.sh` uses BuildKit's parser and Go client rather than
Bollard's solve path:

```bash
./independent_solve.sh <buildkit-container> \
  ../../llb/testdata/golden/mkfile.llb.pb /tmp/llb-output
```
