// Package version reports the build version.
package version

import "runtime/debug"

// version is set at link time:
//
//	-ldflags "-X github.com/henry40408/liftlog/internal/version.version=$GIT_VERSION"
//
// Docker builds have no .git directory, so they pass it in.
var version string

// Version returns the injected version, else the VCS-stamped module version,
// else "dev".
func Version() string {
	if version != "" {
		return version
	}
	return buildInfoVersion()
}

func buildInfoVersion() string {
	info, ok := debug.ReadBuildInfo()
	if !ok {
		return "dev"
	}
	v := info.Main.Version
	if v == "" || v == "(devel)" {
		return "dev"
	}
	return v
}
