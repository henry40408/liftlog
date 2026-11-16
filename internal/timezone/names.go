package timezone

import "strings"

// Names lists every accepted zone name in the order of zones.txt, for the
// settings page's suggestions.
func Names() []string {
	var names []string
	for name := range strings.SplitSeq(zonesFile, "\n") {
		name = strings.TrimSpace(name)
		if name != "" {
			names = append(names, name)
		}
	}
	return names
}
