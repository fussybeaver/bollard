// go-parity generates golden LLB protobuf files from Go's moby/buildkit
// client/llb package. The output is committed to ../golden/ and consumed by
// Rust parity tests in ../../tests/parity.rs.
//
// Run from this directory:
//
//	go run main.go
package main

import (
	"context"
	"fmt"
	"os"

	"github.com/moby/buildkit/client/llb"
)

func main() {
	ctx := context.Background()
	outDir := "../golden"

	cases := []struct {
		name  string
		state llb.State
	}{
		{"image_run", imageRun()},
		{"merge", merge()},
		{"copy_all_flags", copyAllFlags()},
		{"mkdir_parents", mkdirParents()},
		{"mkfile", mkfile()},
		{"secret_as_env", secretAsEnv()},
		{"local_all_attrs", localAllAttrs()},
		{"cache_mount_shared", cacheMountShared()},
		{"cache_mount_locked", cacheMountLocked()},
		{"file_operations_chain", fileOperationsChain()},
	}

	for _, c := range cases {
		def, err := c.state.Marshal(ctx, llb.LinuxAmd64)
		if err != nil {
			fmt.Fprintf(os.Stderr, "marshal %s: %v\n", c.name, err)
			os.Exit(1)
		}

		path := fmt.Sprintf("%s/%s.llb.pb", outDir, c.name)
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
		fmt.Println(path)
	}
}

func imageRun() llb.State {
	return llb.Image("alpine:latest").
		Run(llb.Shlex("echo hello")).
		Root()
}

func merge() llb.State {
	return llb.Merge([]llb.State{
		llb.Image("alpine:latest"),
		llb.Image("busybox:latest"),
	})
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

func secretAsEnv() llb.State {
	return llb.Image("alpine:latest").
		Run(
			llb.Shlex("cat /secrets/token"),
			llb.AddSecret("token", llb.SecretID("token"), llb.SecretAsEnv(true), llb.SecretAsEnvName("TOKEN")),
		).
		Root()
}

func localAllAttrs() llb.State {
	return llb.Local("context",
		llb.FollowPaths([]string{"src"}),
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

func cacheMountLocked() llb.State {
	return llb.Image("alpine:latest").
		Run(
			llb.Shlex("echo hello"),
			llb.AddMount("/cache", llb.Scratch(), llb.AsPersistentCacheDir("cache-id", llb.CacheMountLocked)),
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
