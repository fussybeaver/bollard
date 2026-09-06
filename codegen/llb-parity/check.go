package main

import (
	"bytes"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"

	"github.com/moby/buildkit/client/llb"
	"google.golang.org/protobuf/proto"
)

func runCheck(committedDir, generatedDir string) error {
	committed, err := readManifest(committedDir)
	if err != nil {
		return fmt.Errorf("read committed manifest: %w", err)
	}
	generated, err := readManifest(generatedDir)
	if err != nil {
		return fmt.Errorf("read generated manifest: %w", err)
	}

	if committed.GeneratorVersion != generated.GeneratorVersion {
		return fmt.Errorf("generator_version differs: committed=%q generated=%q", committed.GeneratorVersion, generated.GeneratorVersion)
	}
	if committed.BuildKitVersion != generated.BuildKitVersion {
		return fmt.Errorf("buildkit_version differs: committed=%q generated=%q", committed.BuildKitVersion, generated.BuildKitVersion)
	}
	if len(committed.Fixtures) != len(generated.Fixtures) {
		return fmt.Errorf("fixture count differs: committed=%d generated=%d", len(committed.Fixtures), len(generated.Fixtures))
	}
	if err := verifyManifestFiles(committedDir, committed); err != nil {
		return fmt.Errorf("verify committed fixtures: %w", err)
	}
	if err := verifyManifestFiles(generatedDir, generated); err != nil {
		return fmt.Errorf("verify generated fixtures: %w", err)
	}

	for i, cf := range committed.Fixtures {
		gf := generated.Fixtures[i]
		if cf.File != gf.File {
			return fmt.Errorf("fixture %d file differs: committed=%q generated=%q", i, cf.File, gf.File)
		}
		if cf.Fixture != gf.Fixture {
			return fmt.Errorf("fixture %d name differs: committed=%q generated=%q", i, cf.Fixture, gf.Fixture)
		}
		if cf.Platform != gf.Platform {
			return fmt.Errorf("fixture %d platform differs: committed=%q generated=%q", i, cf.Platform, gf.Platform)
		}
		if cf.SHA256 != gf.SHA256 {
			return fmt.Errorf("fixture %s bytes differ: committed sha256=%s generated sha256=%s", cf.File, cf.SHA256, gf.SHA256)
		}

		committedPath := filepath.Join(committedDir, cf.File)
		generatedPath := filepath.Join(generatedDir, gf.File)
		committedBytes, err := os.ReadFile(committedPath)
		if err != nil {
			return fmt.Errorf("read committed %s: %w", cf.File, err)
		}
		generatedBytes, err := os.ReadFile(generatedPath)
		if err != nil {
			return fmt.Errorf("read generated %s: %w", gf.File, err)
		}
		if !bytes.Equal(committedBytes, generatedBytes) {
			return fmt.Errorf("fixture %s bytes differ despite matching metadata", cf.File)
		}
		sum, err := computeSHA256(committedPath)
		if err != nil {
			return fmt.Errorf("hash committed %s: %w", cf.File, err)
		}
		if sum != cf.SHA256 {
			return fmt.Errorf("committed manifest sha256 mismatch for %s: manifest=%s actual=%s", cf.File, cf.SHA256, sum)
		}
		cdef, err := decodeDefinition(filepath.Join(committedDir, cf.File))
		if err != nil {
			return fmt.Errorf("decode committed %s: %w", cf.File, err)
		}
		gdef, err := decodeDefinition(generatedPath)
		if err != nil {
			return fmt.Errorf("decode generated %s: %w", gf.File, err)
		}
		if !proto.Equal(cdef.ToPB(), gdef.ToPB()) {
			return fmt.Errorf("decoded definitions differ for %s", cf.File)
		}
	}

	return nil
}

func verifyManifestFiles(dir string, m *manifest) error {
	seen := make(map[string]struct{}, len(m.Fixtures))
	for _, fixture := range m.Fixtures {
		if _, exists := seen[fixture.File]; exists {
			return fmt.Errorf("duplicate fixture file %q", fixture.File)
		}
		seen[fixture.File] = struct{}{}
		path := filepath.Join(dir, fixture.File)
		sum, err := computeSHA256(path)
		if err != nil {
			return fmt.Errorf("hash %s: %w", fixture.File, err)
		}
		if sum != fixture.SHA256 {
			return fmt.Errorf("manifest sha256 mismatch for %s: manifest=%s actual=%s", fixture.File, fixture.SHA256, sum)
		}
	}

	entries, err := os.ReadDir(dir)
	if err != nil {
		return err
	}
	for _, entry := range entries {
		if filepath.Ext(entry.Name()) == ".pb" {
			if _, exists := seen[entry.Name()]; !exists {
				return fmt.Errorf("fixture %s is not listed in manifest", entry.Name())
			}
		}
	}
	return nil
}

func computeSHA256(path string) (string, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		return "", err
	}
	sum := sha256.Sum256(data)
	return hex.EncodeToString(sum[:]), nil
}

func readManifest(dir string) (*manifest, error) {
	path := filepath.Join(dir, "manifest.json")
	data, err := os.ReadFile(path)
	if err != nil {
		return nil, err
	}
	var m manifest
	if err := json.Unmarshal(data, &m); err != nil {
		return nil, err
	}
	return &m, nil
}

func decodeDefinition(path string) (*llb.Definition, error) {
	f, err := os.Open(path)
	if err != nil {
		return nil, err
	}
	defer f.Close()
	return llb.ReadFrom(f)
}
