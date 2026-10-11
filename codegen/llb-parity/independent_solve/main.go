// independent_solve submits an existing LLB protobuf through BuildKit's Go
// client without using Bollard's SolveDefinition path.
package main

import (
	"context"
	"flag"
	"fmt"
	"os"
	"time"

	"github.com/moby/buildkit/client"
	_ "github.com/moby/buildkit/client/connhelper/dockercontainer"
	"github.com/moby/buildkit/client/llb"
)

func main() {
	var address string
	var fixture string
	var output string
	var timeout time.Duration
	var omitSource bool
	flag.StringVar(&address, "address", "", "BuildKit address, for example docker-container://buildkit")
	flag.StringVar(&fixture, "fixture", "", "path to an LLB protobuf definition")
	flag.StringVar(&output, "output", "", "local output directory")
	flag.DurationVar(&timeout, "timeout", 2*time.Minute, "solve timeout")
	flag.BoolVar(&omitSource, "omit-source", false, "omit the definition source map before solving")
	flag.Parse()

	if address == "" || fixture == "" || output == "" {
		flag.Usage()
		os.Exit(2)
	}

	definitionFile, err := os.Open(fixture)
	if err != nil {
		fatal("open fixture", err)
	}
	definition, err := llb.ReadFrom(definitionFile)
	closeErr := definitionFile.Close()
	if err != nil {
		fatal("decode fixture", err)
	}
	if closeErr != nil {
		fatal("close fixture", closeErr)
	}
	if omitSource {
		definition.Source = nil
	}
	printDefinitionSummary(definition, omitSource)

	ctx, cancel := context.WithTimeout(context.Background(), timeout)
	defer cancel()

	buildkit, err := client.New(ctx, address)
	if err != nil {
		fatal("connect to BuildKit", err)
	}
	defer buildkit.Close()

	_, err = buildkit.Solve(ctx, definition, client.SolveOpt{
		Exports: []client.ExportEntry{{
			Type:      client.ExporterLocal,
			OutputDir: output,
		}},
	}, nil)
	if err != nil {
		fatal("solve fixture", err)
	}

	fmt.Printf("independent_solve=PASS fixture=%s address=%s output=%s\n", fixture, address, output)
}

func fatal(operation string, err error) {
	fmt.Fprintf(os.Stderr, "independent_solve=FAIL operation=%s error=%v\n", operation, err)
	os.Exit(1)
}

func printDefinitionSummary(definition *llb.Definition, omitSource bool) {
	source := definition.Source
	sourcePresent := source != nil
	infoCount := 0
	locationCount := 0
	locationKeyCount := 0
	nilLocationCount := 0
	if source != nil {
		infoCount = len(source.Infos)
		for _, locations := range source.Locations {
			locationKeyCount++
			if locations != nil {
				locationCount += len(locations.Locations)
			} else {
				nilLocationCount++
			}
		}
	}
	fmt.Printf(
		"definition_ops=%d metadata=%d source_present=%t source_infos=%d source_location_keys=%d source_locations=%d nil_source_locations=%d source_omitted=%t\n",
		len(definition.Def),
		len(definition.Metadata),
		sourcePresent,
		infoCount,
		locationKeyCount,
		locationCount,
		nilLocationCount,
		omitSource,
	)
}
