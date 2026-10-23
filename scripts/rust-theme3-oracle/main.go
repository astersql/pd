// Copyright 2026 AsterSQL.
// Command rust-theme3-oracle records real Go behavior for Rust compatibility tests.
package main

import "os"

func main() {
	if len(os.Args) != 2 || os.Args[1] != "cluster_paths" {
		panic("usage: rust-theme3-oracle cluster_paths")
	}
	clusterPaths()
}
