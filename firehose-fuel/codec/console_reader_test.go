// Copyright 2021 dfuse Platform Inc.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//      http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

package codec

import (
	"bytes"
	"encoding/json"
	"io"
	"io/ioutil"
	"os"
	"os/exec"
	"strings"
	"testing"

	firecore "github.com/streamingfast/firehose-core"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestParseFromFile(t *testing.T) {
	tests := []struct {
		firehoseLogsFile string
		expectedPanicErr error
	}{
		{"testdata/full.firelog", nil},
	}

	for _, test := range tests {
		t.Run(strings.Replace(test.firehoseLogsFile, "testdata/", "", 1), func(t *testing.T) {
			defer func() {
				if r := recover(); r != nil {
					require.Equal(t, test.expectedPanicErr, r)
				}
			}()

			cr := testFileConsoleReader(t, test.firehoseLogsFile)
			buf := &bytes.Buffer{}
			buf.Write([]byte("["))

			for first := true; true; first = false {
				block, err := cr.readBlock()
				if err == io.EOF {
					break
				}
				require.NoError(t, err)

				if !first {
					buf.Write([]byte(","))
				}

				// FIXMME: jsonpb needs to be updated to latest version of used gRPC
				//         elements. We are disaligned and using that breaks now.
				//         Needs to check what is the latest way to properly serialize
				//         Proto generated struct to JSON.
				// value, err := jsonpb.MarshalIndentToString(v, "  ")
				// require.NoError(t, err)

				value, err := json.MarshalIndent(block, "", "  ")
				require.NoError(t, err)

				buf.Write(value)
			}

			if len(buf.Bytes()) != 0 {
				buf.Write([]byte("\n"))
			}

			buf.Write([]byte("]"))

			goldenFile := test.firehoseLogsFile + ".golden.json"
			if os.Getenv("GOLDEN_UPDATE") == "true" {
				ioutil.WriteFile(goldenFile, buf.Bytes(), os.ModePerm)
			}

			cnt, err := ioutil.ReadFile(goldenFile)
			require.NoError(t, err)

			if !assert.Equal(t, string(cnt), buf.String()) {
				t.Error("previous diff:\n" + unifiedDiff(t, cnt, buf.Bytes()))
			}
		})
	}
}

func testFileConsoleReader(t *testing.T, filename string) *ConsoleReader {
	t.Helper()

	fl, err := os.Open(filename)
	require.NoError(t, err)

	cr := testReaderConsoleReader(t, make(chan string, 10000), func() { fl.Close() })

	go cr.processData(fl)

	return cr
}

func testReaderConsoleReader(t *testing.T, lines chan string, closer func()) *ConsoleReader {
	t.Helper()

	l := &ConsoleReader{
		lines:        lines,
		blockEncoder: firecore.NewBlockEncoder(),
		close:        closer,
		logger:       zlog,
	}

	return l
}

func unifiedDiff(t *testing.T, cnt1, cnt2 []byte) string {
	file1 := "/tmp/gotests-linediff-1"
	file2 := "/tmp/gotests-linediff-2"
	err := ioutil.WriteFile(file1, cnt1, 0600)
	require.NoError(t, err)

	err = ioutil.WriteFile(file2, cnt2, 0600)
	require.NoError(t, err)

	cmd := exec.Command("diff", "-u", file1, file2)
	out, _ := cmd.Output()

	return string(out)
}
