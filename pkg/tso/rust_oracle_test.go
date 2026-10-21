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

package tso

import (
	"context"
	"encoding/json"
	"os"
	"testing"
	"time"

	"github.com/stretchr/testify/require"
)

// TestRustOracleVectors fixes the classic Go bit/guard behavior consumed by the
// Rust test. Physical times are deterministic milliseconds, with overflow's
// periodic update driven explicitly so the host wall clock does not affect it.
func TestRustOracleVectors(t *testing.T) {
	re := require.New(t)
	data, err := os.ReadFile("compat_test.json")
	re.NoError(err)
	var vectors map[string][2]int64
	re.NoError(json.Unmarshal(data, &vectors))
	oracle := &timestampOracle{tsoMux: &tsoObject{physical: time.UnixMilli(1000)}, metrics: newTSOMetrics("rust-oracle")}
	check := func(name string, count int64) {
		physical, logical := oracle.generateTSO(context.Background(), count)
		expected := vectors[name]
		actual := [2]int64{physical, logical}
		re.Equal(expected, actual, name)
		t.Logf("%s physical=%d logical=%d", name, physical, logical)
	}
	check("concurrent", 320)
	oracle.setTSOPhysical(time.UnixMilli(500), mustInitialized())
	check("rollback", 1)
	check("before_overflow", maxLogical-322)
	_, logical := oracle.generateTSO(context.Background(), 2)
	re.True(overflowedLogical(logical))
	oracle.setTSOPhysical(time.UnixMilli(1001), mustInitialized(), mustOverflowed())
	check("overflow", 2)
}
