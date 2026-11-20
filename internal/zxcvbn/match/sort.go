package match

import (
	"cmp"
	"slices"
)

func Sort(matches []*Match) {
	slices.SortStableFunc(matches, func(a, b *Match) int {
		return cmp.Or(cmp.Compare(a.I, b.I), cmp.Compare(a.J, b.J))
	})
}
