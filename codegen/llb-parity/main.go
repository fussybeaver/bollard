// go-parity generates golden LLB protobuf files from Go's moby/buildkit
// client/llb package. The output is consumed by Rust parity tests in
// ../../llb/tests/parity.rs.
//
// Run from this directory with an explicit output directory:
//
//	go run . -out /tmp/llb-golden
package main

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"flag"
	"fmt"
	"os"
	"path/filepath"
	"runtime/debug"

	"github.com/moby/buildkit/client/llb"
	ocispecs "github.com/opencontainers/image-spec/specs-go/v1"
)

// manifestEntry records one golden fixture and its content hash.
type manifestEntry struct {
	File     string `json:"file"`
	Fixture  string `json:"fixture"`
	SHA256   string `json:"sha256"`
	Platform string `json:"platform"`
}

// manifest is written beside the golden files so CI can verify that every
// committed golden was produced by this generator with the pinned BuildKit
// module version. The generator version is the source version of this file and
// must be updated whenever main.go changes in a way that affects output.
type manifest struct {
	GeneratorVersion string          `json:"generator_version"`
	BuildKitVersion  string          `json:"buildkit_version"`
	Fixtures         []manifestEntry `json:"fixtures"`
}

const generatorVersion = "1"

// fixtureCase bundles the user-facing state with the marshal constraints that
// the corresponding Rust test uses.
type fixtureCase struct {
	name     string
	state    func() llb.State
	marshal  []llb.ConstraintsOpt
	platform string
}

func main() {
	var outDir string
	var checkDir string
	var againstDir string
	flag.StringVar(&outDir, "out", "", "output directory for golden .llb.pb files")
	flag.StringVar(&checkDir, "check", "", "reference golden directory to verify")
	flag.StringVar(&againstDir, "against", "", "freshly generated directory to check against")
	flag.Parse()

	if checkDir != "" {
		if outDir != "" {
			fmt.Fprintln(os.Stderr, "cannot use -out with -check")
			os.Exit(1)
		}
		if againstDir == "" {
			fmt.Fprintln(os.Stderr, "-against is required when -check is used")
			os.Exit(1)
		}
		if err := runCheck(checkDir, againstDir); err != nil {
			fmt.Fprintf(os.Stderr, "check failed: %v\n", err)
			os.Exit(1)
		}
		fmt.Println("GOLDEN CHECK PASSED")
		return
	}

	if outDir == "" {
		fmt.Fprintln(os.Stderr, "-out is required when generating fixtures")
		os.Exit(2)
	}

	ctx := context.Background()

	cases := []fixtureCase{
		{"image_run", imageRun, defaultPlatform(), "linux/amd64"},
		{"image_resolve_force_pull", imageResolveForcePull, defaultPlatform(), "linux/amd64"},
		{"image_resolve_prefer_local", imageResolvePreferLocal, defaultPlatform(), "linux/amd64"},
		{"platform_arm64", imageRun, []llb.ConstraintsOpt{llb.LinuxArm64}, "linux/arm64"},
		{"platform_arm_v7", imageRun, []llb.ConstraintsOpt{llb.LinuxArmhf}, "linux/arm/v7"},
		{"platform_image_override", platformImageOverride, defaultPlatform(), "linux/amd64"},
		{"platform_state_override", platformStateOverride, defaultPlatform(), "linux/amd64"},
		{"platform_mixed", platformMixed, defaultPlatform(), "linux/amd64"},
		{"platform_shared_subgraph", platformSharedSubgraph, defaultPlatform(), "linux/amd64"},
		{"exec_default_meta", execDefaultMeta, defaultPlatform(), "linux/amd64"},
		{"exec_custom_name_ignore_cache", execCustomNameIgnoreCache, defaultPlatform(), "linux/amd64"},
		{"merge", merge, defaultPlatform(), "linux/amd64"},
		{"merge_custom_name", mergeCustomName, defaultPlatform(), "linux/amd64"},
		{"copy_all_flags", copyAllFlags, defaultPlatform(), "linux/amd64"},
		{"mkdir_parents", mkdirParents, defaultPlatform(), "linux/amd64"},
		{"mkfile", mkfile, defaultPlatform(), "linux/amd64"},
		{"rm_wildcard", rmWildcard, defaultPlatform(), "linux/amd64"},
		{"symlink", symlinkFixture, defaultPlatform(), "linux/amd64"},
		{"file_ops_mkdir", fileOpsMkdir, defaultPlatform(), "linux/amd64"},
		{"file_ops_mkfile", fileOpsMkfile, defaultPlatform(), "linux/amd64"},
		{"file_ops_symlink", fileOpsSymlink, defaultPlatform(), "linux/amd64"},
		{"file_ops_copy", fileOpsCopy, defaultPlatform(), "linux/amd64"},
		{"file_ops_rm", fileOpsRm, defaultPlatform(), "linux/amd64"},
		{"file_ops_rm_allow_not_found", fileOpsRmAllowNotFound, defaultPlatform(), "linux/amd64"},
		{"secret_file_default", secretFileDefault, defaultPlatform(), "linux/amd64"},
		{"secret_file_optional", secretFileOptional, defaultPlatform(), "linux/amd64"},
		{"secret_file_permissions", secretFilePermissions, defaultPlatform(), "linux/amd64"},
		{"secret_as_env", secretAsEnv, defaultPlatform(), "linux/amd64"},
		{"secret_env_explicit_name", secretEnvExplicitName, defaultPlatform(), "linux/amd64"},
		{"local_all_attrs", localAllAttrs, defaultPlatform(), "linux/amd64"},
		{"cache_mount_shared", cacheMountShared, defaultPlatform(), "linux/amd64"},
		{"cache_mount_private", cacheMountPrivate, defaultPlatform(), "linux/amd64"},
		{"cache_mount_locked", cacheMountLocked, defaultPlatform(), "linux/amd64"},
		{"multi_mount_ordering", multiMountOrdering, defaultPlatform(), "linux/amd64"},
		{"file_operations_chain", fileOperationsChain, defaultPlatform(), "linux/amd64"},
		{"differential_merge_alpine", differentialMergeAlpine, defaultPlatform(), "linux/amd64"},
		{"differential_file_secret", differentialFileSecret, defaultPlatform(), "linux/amd64"},
		{"differential_env_secret", differentialEnvSecret, defaultPlatform(), "linux/amd64"},
		{"differential_file_operations_allow_not_found", differentialFileOperationsAllowNotFound, defaultPlatform(), "linux/amd64"},
		{"scratch_direct", scratchDirect, defaultPlatform(), "linux/amd64"},
		{"scratch_exec_root", scratchExecRoot, defaultPlatform(), "linux/amd64"},
		{"scratch_bind_mount", scratchBindMount, defaultPlatform(), "linux/amd64"},
	}

	if err := os.MkdirAll(outDir, 0o755); err != nil {
		fmt.Fprintf(os.Stderr, "mkdir %s: %v\n", outDir, err)
		os.Exit(1)
	}

	m := manifest{
		GeneratorVersion: generatorVersion,
		BuildKitVersion:  buildKitVersion(),
	}

	for _, c := range cases {
		def, err := c.state().Marshal(ctx, c.marshal...)
		if err != nil {
			fmt.Fprintf(os.Stderr, "marshal %s: %v\n", c.name, err)
			os.Exit(1)
		}

		path := filepath.Join(outDir, fmt.Sprintf("%s.llb.pb", c.name))
		f, err := os.Create(path)
		if err != nil {
			fmt.Fprintf(os.Stderr, "create %s: %v\n", path, err)
			os.Exit(1)
		}
		if err := llb.WriteTo(def, f); err != nil {
			fmt.Fprintf(os.Stderr, "write %s: %v\n", path, err)
			os.Exit(1)
		}
		if err := f.Close(); err != nil {
			fmt.Fprintf(os.Stderr, "close %s: %v\n", path, err)
			os.Exit(1)
		}

		data, err := os.ReadFile(path)
		if err != nil {
			fmt.Fprintf(os.Stderr, "read %s: %v\n", path, err)
			os.Exit(1)
		}
		sum := sha256.Sum256(data)
		m.Fixtures = append(m.Fixtures, manifestEntry{
			File:     filepath.Base(path),
			Fixture:  c.name,
			SHA256:   hex.EncodeToString(sum[:]),
			Platform: c.platform,
		})
		fmt.Println(path)
	}

	manifestPath := filepath.Join(outDir, "manifest.json")
	mf, err := os.Create(manifestPath)
	if err != nil {
		fmt.Fprintf(os.Stderr, "create %s: %v\n", manifestPath, err)
		os.Exit(1)
	}
	enc := json.NewEncoder(mf)
	enc.SetIndent("", "  ")
	if err := enc.Encode(m); err != nil {
		fmt.Fprintf(os.Stderr, "encode %s: %v\n", manifestPath, err)
		os.Exit(1)
	}
	if err := mf.Close(); err != nil {
		fmt.Fprintf(os.Stderr, "close %s: %v\n", manifestPath, err)
		os.Exit(1)
	}
	fmt.Println(manifestPath)
}

func defaultPlatform() []llb.ConstraintsOpt {
	return []llb.ConstraintsOpt{llb.LinuxAmd64}
}

func buildKitVersion() string {
	info, ok := debug.ReadBuildInfo()
	if !ok {
		return "unknown"
	}
	for _, dep := range info.Deps {
		if dep.Path == "github.com/moby/buildkit" {
			return dep.Version
		}
	}
	return "unknown"
}

func imageRun() llb.State {
	return llb.Image("alpine:latest").
		Run(llb.Shlex("echo hello")).
		Root()
}

func imageResolveForcePull() llb.State {
	return llb.Image("alpine:latest", llb.ResolveModeForcePull).
		Run(llb.Shlex("echo hello")).
		Root()
}

func imageResolvePreferLocal() llb.State {
	return llb.Image("alpine:latest", llb.ResolveModePreferLocal).
		Run(llb.Shlex("echo hello")).
		Root()
}

func platformImageOverride() llb.State {
	return llb.Image("alpine:latest", llb.LinuxArm64).
		Run(llb.Shlex("echo image override")).
		Root()
}

func platformStateOverride() llb.State {
	return llb.Image("alpine:latest").
		Platform(ocispecs.Platform{OS: "linux", Architecture: "arm64"}).
		Run(llb.Shlex("echo state override")).
		Root()
}

func platformMixed() llb.State {
	main := llb.Image("image1:latest").Run(llb.Shlex("cmd-main"))
	sub := llb.Image("image2:latest", llb.LinuxArmel).Run(llb.Shlex("cmd-sub")).Root()
	main.AddMount("/mnt", sub)
	return main.Root()
}

func platformSharedSubgraph() llb.State {
	shared := llb.Image("shared:latest", llb.LinuxArm64).
		Run(llb.Shlex("cmd-shared")).
		Root()
	return llb.Image("main:latest").
		Run(
			llb.Shlex("cmd-main"),
			llb.AddMount("/left", shared),
			llb.AddMount("/right", shared),
		).
		Root()
}

func execDefaultMeta() llb.State {
	return llb.Image("alpine:latest").
		Run(llb.Shlex("true")).
		Root()
}

func execCustomNameIgnoreCache() llb.State {
	return llb.Image("alpine:latest").
		Run(llb.Shlex("echo hello"), llb.WithCustomName("named exec"), llb.IgnoreCache).
		Root()
}

func merge() llb.State {
	return llb.Merge([]llb.State{
		llb.Image("alpine:latest"),
		llb.Image("busybox:latest"),
	})
}

func mergeCustomName() llb.State {
	return llb.Merge([]llb.State{
		llb.Image("alpine:latest"),
		llb.Image("busybox:latest"),
	}, llb.WithCustomName("merged"))
}

func copyAllFlags() llb.State {
	base := llb.Image("alpine:latest")
	src := llb.Image("busybox:latest")
	return base.File(llb.Copy(src, "/src", "/dst", &llb.CopyInfo{
		CreateDestPath:      true,
		FollowSymlinks:      true,
		CopyDirContentsOnly: true,
		AllowWildcard:       true,
		AllowEmptyWildcard:  true,
		ExcludePatterns:     []string{"*.tmp"},
	}))
}

func mkdirParents() llb.State {
	return llb.Scratch().File(llb.Mkdir("/tmp", 0o755, llb.WithParents(true)))
}

func mkfile() llb.State {
	return llb.Scratch().File(llb.Mkfile("/hello", 0o644, []byte("world")))
}

func rmWildcard() llb.State {
	return llb.Scratch().File(llb.Rm("/tmp/*", llb.WithAllowWildcard(true)))
}

func symlinkFixture() llb.State {
	return llb.Scratch().File(llb.Symlink("/target", "/link"))
}

func secretFileDefault() llb.State {
	return llb.Image("alpine:latest").
		Run(
			llb.Shlex("cat /run/secrets/token"),
			llb.AddSecret("token", llb.SecretID("token")),
		).
		Root()
}

func secretFileOptional() llb.State {
	return llb.Image("alpine:latest").
		Run(
			llb.Shlex("cat /run/secrets/token"),
			llb.AddSecret("token", llb.SecretID("token"), llb.SecretOptional),
		).
		Root()
}

func secretFilePermissions() llb.State {
	return llb.Image("alpine:latest").
		Run(
			llb.Shlex("cat /run/secrets/license"),
			llb.AddSecret(
				"/run/secrets/license",
				llb.SecretID("license"),
				llb.SecretFileOpt(1000, 1001, 0o440),
			),
		).
		Root()
}

func secretAsEnv() llb.State {
	return llb.Image("alpine:latest").
		Run(
			llb.Shlex("cat /secrets/token"),
			llb.AddSecret("token", llb.SecretID("token"), llb.SecretAsEnv(true)),
		).
		Root()
}

func secretEnvExplicitName() llb.State {
	return llb.Image("alpine:latest").
		Run(
			llb.Shlex("cat /secrets/token"),
			llb.AddSecret("mysecret", llb.SecretID("mysecret"), llb.SecretAsEnv(true), llb.SecretAsEnvName("MY_SECRET")),
		).
		Root()
}

func localAllAttrs() llb.State {
	return llb.Local("context",
		llb.FollowPaths([]string{"src"}),
		llb.IncludePatterns([]string{"*.go"}),
		llb.ExcludePatterns([]string{"*_test.go"}),
		llb.SessionID("sess"),
		llb.SharedKeyHint("hint"),
		llb.LocalUniqueID("unique"),
	)
}

func cacheMountShared() llb.State {
	return llb.Image("alpine:latest").
		Run(
			llb.Shlex("echo hello"),
			llb.AddMount("/cache", llb.Scratch(), llb.AsPersistentCacheDir("cache-id", llb.CacheMountShared)),
		).
		Root()
}

func cacheMountPrivate() llb.State {
	return llb.Image("alpine:latest").
		Run(
			llb.Shlex("echo hello"),
			llb.AddMount("/cache", llb.Scratch(), llb.AsPersistentCacheDir("cache-id", llb.CacheMountPrivate)),
		).
		Root()
}

func cacheMountLocked() llb.State {
	return llb.Image("alpine:latest").
		Run(
			llb.Shlex("echo hello"),
			llb.AddMount("/cache", llb.Scratch(), llb.AsPersistentCacheDir("cache-id", llb.CacheMountLocked)),
		).
		Root()
}

func multiMountOrdering() llb.State {
	// Mounts are deliberately added out of alphabetical order. Go's ExecOp
	// Marshal sorts them by target path, so the serialized op should contain
	// /a, /b, /c regardless of insertion order.
	return llb.Image("alpine:latest").
		Run(
			llb.Shlex("echo hello"),
			llb.AddMount("/c", llb.Scratch(), llb.AsPersistentCacheDir("cache-c", llb.CacheMountShared)),
			llb.AddMount("/a", llb.Scratch(), llb.AsPersistentCacheDir("cache-a", llb.CacheMountShared)),
			llb.AddMount("/b", llb.Scratch(), llb.AsPersistentCacheDir("cache-b", llb.CacheMountShared)),
		).
		Root()
}

func fileOperationsChain() llb.State {
	base := llb.Scratch()
	withDir := base.File(llb.Mkdir("/app", 0o755, llb.WithParents(true)))
	withFile := withDir.File(llb.Mkfile("/app/config.toml", 0o644, []byte("[server]\nhost = \"0.0.0.0\"\n")))
	withSymlink := withFile.File(llb.Symlink("/app/config.toml", "/app/current-config"))
	withCopy := withSymlink.File(llb.Copy(withSymlink, "/app/config.toml", "/app/config.toml.bak", &llb.CopyInfo{
		CreateDestPath: true,
	}))
	return withCopy.File(llb.Rm("/app/current-config"))
}

func fileOpsMkdir() llb.State {
	return llb.Scratch().File(llb.Mkdir("/app", 0o755, llb.WithParents(true)))
}

func fileOpsMkfile() llb.State {
	base := fileOpsMkdir()
	return base.File(llb.Mkfile("/app/config.toml", 0o644, []byte("[server]\nhost = \"0.0.0.0\"\n")))
}

func fileOpsSymlink() llb.State {
	base := fileOpsMkfile()
	return base.File(llb.Symlink("/app/config.toml", "/app/current-config"))
}

func fileOpsCopy() llb.State {
	base := fileOpsMkfile()
	return base.File(llb.Copy(base, "/app/config.toml", "/app/config.toml.bak", &llb.CopyInfo{
		CreateDestPath: true,
	}))
}

func fileOpsRm() llb.State {
	base := fileOpsSymlink()
	return base.File(llb.Rm("/app/current-config"))
}

func fileOpsRmAllowNotFound() llb.State {
	base := fileOpsSymlink()
	return base.File(llb.Rm("/app/current-config", llb.WithAllowNotFound(true)))
}

func differentialMergeAlpine() llb.State {
	return llb.Merge([]llb.State{
		llb.Image("alpine:latest"),
		llb.Image("alpine:latest"),
	}).Run(llb.Shlex("sh -c 'echo differential > /differential'"), llb.IgnoreCache).Root()
}

func differentialFileSecret() llb.State {
	target := "/run/secrets/token"
	return llb.Image("alpine:latest").
		Run(
			llb.Shlex("sh -c 'sha256sum /run/secrets/token > /derived'"),
			llb.AddSecretWithDest("token", &target, llb.SecretID("token")),
		).
		Root()
}

func differentialEnvSecret() llb.State {
	return llb.Image("alpine:latest").
		Run(
			llb.Shlex("sh -c 'printf %s \"$MY_SECRET\" | sha256sum > /derived'"),
			llb.AddSecret("mysecret", llb.SecretID("mysecret"), llb.SecretAsEnv(true), llb.SecretAsEnvName("MY_SECRET")),
		).
		Root()
}

func differentialFileOperationsAllowNotFound() llb.State {
	base := fileOpsSymlink()
	return base.File(llb.Rm("/app/current-config", llb.WithAllowNotFound(true)))
}

func scratchDirect() llb.State {
	return llb.Scratch()
}

func scratchExecRoot() llb.State {
	return llb.Scratch().Run(llb.Shlex("echo hello")).Root()
}

func scratchBindMount() llb.State {
	return llb.Image("alpine:latest").
		Run(
			llb.Shlex("echo hello"),
			llb.AddMount("/scratch", llb.Scratch()),
		).
		Root()
}
