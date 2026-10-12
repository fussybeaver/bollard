# BuildKit LLB parity oracle

This pinned Go module generates the BuildKit LLB definitions consumed by the
`bollard-llb` compatibility tests. The generated definitions remain owned by
the crate under `../../llb/testdata/golden/`.

## Regenerate

From this directory, run:

```bash
./regenerate.sh
```

The script generates into two temporary directories, verifies that the pinned
generator produces byte-identical output twice, removes stale fixture files,
and then copies the new results into `llb/testdata/golden/`. The generator
binary is not checked in. CI separately checks the generated output against
the committed goldens and manifest.

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

CircleCI runs this check in the parallel `test_llb_independent_solve` job. The
job reads the image, version and commit from the protobuf provenance lock,
starts a temporary privileged BuildKit container, validates the runtime
identity, parses and solves the `mkfile` golden with the independent Go client,
and checks the exported file contents and mode. It does not depend on the
Bollard BuildKit or golden jobs.
