// Copyright 2026 TiKV Project Authors.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

package storelimit

import (
	"encoding/json"
	"os"
	"testing"

	"github.com/stretchr/testify/require"
)

func TestRustCoreTask02Oracle(t *testing.T) {
	var fixture struct {
		TypeValues []int64         `json:"type_values"`
		Names      []string        `json:"names"`
		Expected   json.RawMessage `json:"expected"`
	}
	data, err := os.ReadFile("rust-fixtures/core_task_02.json")
	require.NoError(t, err)
	require.NoError(t, json.Unmarshal(data, &fixture))
	type lookup struct {
		Value Type `json:"value"`
		Found bool `json:"found"`
	}
	actual := struct {
		TypeNames            []string `json:"type_names"`
		Lookups              []lookup `json:"lookups"`
		TypeCount            int      `json:"type_count"`
		RegionInfluence      []int64  `json:"region_influence"`
		SmallRegionInfluence []int64  `json:"small_region_influence"`
		SmallRegionThreshold int64    `json:"small_region_threshold"`
		Unlimited            float64  `json:"unlimited"`
		Versions             []string `json:"versions"`
	}{TypeCount: int(storeLimitTypeLen), RegionInfluence: RegionInfluence,
		SmallRegionInfluence: SmallRegionInfluence, SmallRegionThreshold: SmallRegionThreshold,
		Unlimited: Unlimited, Versions: []string{VersionV1, VersionV2}}
	for _, value := range fixture.TypeValues {
		actual.TypeNames = append(actual.TypeNames, Type(value).String())
	}
	for _, name := range fixture.Names {
		value, found := TypeNameValue[name]
		actual.Lookups = append(actual.Lookups, lookup{value, found})
	}
	encoded, err := json.Marshal(actual)
	require.NoError(t, err)
	t.Logf("Go oracle: %s", encoded)
	require.JSONEq(t, string(fixture.Expected), string(encoded))
}
