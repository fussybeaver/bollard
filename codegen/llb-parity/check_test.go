package main

import (
	"context"
	"encoding/json"
	"os"
	"path/filepath"
	"testing"

	"github.com/moby/buildkit/client/llb"
	"google.golang.org/protobuf/proto"
)

func TestRunCheckAcceptsIdenticalFixtures(t *testing.T) {
	definition, err := imageRun().Marshal(context.Background(), defaultPlatform()...)
	if err != nil {
		t.Fatal(err)
	}
	data, err := proto.MarshalOptions{Deterministic: true}.Marshal(definition.ToPB())
	if err != nil {
		t.Fatal(err)
	}

	committed := t.TempDir()
	generated := t.TempDir()
	writeTestFixture(t, committed, data)
	writeTestFixture(t, generated, data)

	if err := runCheck(committed, generated); err != nil {
		t.Fatalf("identical fixtures should pass: %v", err)
	}
}

func TestRunCheckRejectsSemanticEqualByteDrift(t *testing.T) {
	definition, err := imageRun().Marshal(context.Background(), defaultPlatform()...)
	if err != nil {
		t.Fatal(err)
	}
	deterministic, err := proto.MarshalOptions{Deterministic: true}.Marshal(definition.ToPB())
	if err != nil {
		t.Fatal(err)
	}

	var nondeterministic []byte
	for attempt := 0; attempt < 100; attempt++ {
		nondeterministic, err = proto.MarshalOptions{Deterministic: false}.Marshal(definition.ToPB())
		if err != nil {
			t.Fatal(err)
		}
		if string(nondeterministic) != string(deterministic) {
			break
		}
	}
	if string(nondeterministic) == string(deterministic) {
		t.Skip("protobuf map encoding did not produce an alternate valid wire order")
	}

	committed := t.TempDir()
	generated := t.TempDir()
	writeTestFixture(t, committed, deterministic)
	writeTestFixture(t, generated, nondeterministic)

	if err := runCheck(committed, generated); err == nil {
		t.Fatal("semantically equal but byte-different fixtures should fail")
	}
}

func TestRunCheckRejectsStaleManifestHash(t *testing.T) {
	definition, err := llb.Scratch().Marshal(context.Background(), defaultPlatform()...)
	if err != nil {
		t.Fatal(err)
	}
	data, err := proto.MarshalOptions{Deterministic: true}.Marshal(definition.ToPB())
	if err != nil {
		t.Fatal(err)
	}

	committed := t.TempDir()
	generated := t.TempDir()
	writeTestFixture(t, committed, data)
	writeTestFixture(t, generated, data)

	manifestPath := filepath.Join(committed, "manifest.json")
	manifestData, err := os.ReadFile(manifestPath)
	if err != nil {
		t.Fatal(err)
	}
	var fixtureManifest manifest
	if err := json.Unmarshal(manifestData, &fixtureManifest); err != nil {
		t.Fatal(err)
	}
	fixtureManifest.Fixtures[0].SHA256 = "stale"
	manifestData, err = json.Marshal(fixtureManifest)
	if err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(manifestPath, manifestData, 0o644); err != nil {
		t.Fatal(err)
	}

	if err := runCheck(committed, generated); err == nil {
		t.Fatal("stale manifest hash should fail")
	}
}

func writeTestFixture(t *testing.T, dir string, data []byte) {
	t.Helper()
	const filename = "fixture.llb.pb"
	if err := os.WriteFile(filepath.Join(dir, filename), data, 0o644); err != nil {
		t.Fatal(err)
	}
	sum, err := computeSHA256(filepath.Join(dir, filename))
	if err != nil {
		t.Fatal(err)
	}
	fixtureManifest := manifest{
		GeneratorVersion: generatorVersion,
		BuildKitVersion:  buildKitVersion(),
		Fixtures: []manifestEntry{{
			File:     filename,
			Fixture:  "fixture",
			SHA256:   sum,
			Platform: "linux/amd64",
		}},
	}
	manifestData, err := json.Marshal(fixtureManifest)
	if err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(dir, "manifest.json"), manifestData, 0o644); err != nil {
		t.Fatal(err)
	}
}
