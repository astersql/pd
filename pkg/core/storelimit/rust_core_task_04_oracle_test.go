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
	"testing/synctest"
	"time"

	"github.com/stretchr/testify/require"

	"github.com/tikv/pd/pkg/core/constant"
)

func TestRustCoreTask04Oracle(t *testing.T) {
	var scenarios []struct {
		Name       string  `json:"name"`
		Rate       float64 `json:"rate"`
		Operations []struct {
			Action string  `json:"action"`
			Typ    Type    `json:"typ"`
			Cost   int64   `json:"cost"`
			Rate   float64 `json:"rate"`
			MS     int64   `json:"ms"`
		} `json:"operations"`
		Expected []bool `json:"expected"`
	}
	data, err := os.ReadFile("rust-fixtures/core_task_04.json")
	require.NoError(t, err)
	require.NoError(t, json.Unmarshal(data, &scenarios))
	for _, scenario := range scenarios {
		t.Run(scenario.Name, func(t *testing.T) {
			synctest.Test(t, func(t *testing.T) {
				limiter := NewStoreRateLimit(scenario.Rate).(*StoreRateLimit)
				actual := make([]bool, 0)
				for _, op := range scenario.Operations {
					switch op.Action {
					case "take":
						actual = append(actual, limiter.Take(op.Cost, op.Typ, constant.Low))
					case "available":
						actual = append(actual, limiter.Available(op.Cost, op.Typ, constant.Low))
					case "reset":
						limiter.Reset(op.Rate, op.Typ)
						expectedRate := op.Rate
						if op.Typ == SendSnapshot {
							expectedRate = scenario.Rate
						}
						require.Equal(t, expectedRate, limiter.Rate(op.Typ))
					case "advance":
						// synctest advances virtual time exactly; no wall-clock sleeping.
						time.Sleep(time.Duration(op.MS) * time.Millisecond)
					default:
						t.Fatalf("unknown action %q", op.Action)
					}
				}
				t.Logf("Go oracle %s: %v", scenario.Name, actual)
				require.NotEmpty(t, scenario.Expected)
				require.Equal(t, scenario.Expected, actual)
			})
		})
	}
}
