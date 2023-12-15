package codec

import (
	"bufio"
	"encoding/hex"
	"fmt"
	"io"
	"strings"

	"github.com/streamingfast/bstream"
	pbfuel "github.com/FuelLabs/firehose-fuel/pb/sf/fuel/type/v1"
	firecore "github.com/streamingfast/firehose-core"
	"github.com/streamingfast/logging"
	"github.com/streamingfast/node-manager/mindreader"
	"go.uber.org/zap"
	"google.golang.org/protobuf/proto"
)

// ConsoleReader is what reads the `fuel-core` output directly
type ConsoleReader struct {
	lines        chan string
	blockEncoder firecore.BlockEncoder
	close        func()

	done chan interface{}

	logger *zap.Logger
	tracer logging.Tracer
}

func NewConsoleReader(lines chan string, blockEncoder firecore.BlockEncoder, logger *zap.Logger, tracer logging.Tracer) (mindreader.ConsolerReader, error) {
	return &ConsoleReader{
		lines:        lines,
		blockEncoder: blockEncoder,
		close:        func() {},
		done:         make(chan interface{}),
		logger:       logger,
		tracer:       tracer,
	}, nil
}

// todo: What should this do?
func (r *ConsoleReader) Done() <-chan interface{} {
	return r.done
}

func (r *ConsoleReader) Close() {
	r.close()
}


func (r *ConsoleReader) readBlock() (out *pbfuel.Block, err error) {
	block, err := r.next()
	if err != nil {
		return nil, err
	}

	return block, nil
}

func (r *ConsoleReader) ReadBlock() (out *bstream.Block, err error) {
	block, err := r.readBlock()
	if err != nil {
		return nil, err
	}

	return r.blockEncoder.Encode(block)
}

const (
	LogPrefix     = "FIRE "
	LogProto	  = "PROTO"
)

func (r *ConsoleReader) next() (out *pbfuel.Block, err error) {
	for line := range r.lines {
		if !strings.HasPrefix(line, LogPrefix) {
			continue
		}

		args := strings.Split(line[len(LogPrefix):], " ")
		if len(args) < 2 {
			return nil, fmt.Errorf("invalid log line %q", line)
		}

		// Order the case from most occurring line prefix to least occurring
		switch args[0] {
		case LogProto:
			block := &pbfuel.Block{}
			bytes, err := hex.DecodeString(args[1])
			if err != nil {
				return nil, fmt.Errorf("invalid encoded block: %w", err)
			}
			if err := proto.Unmarshal(bytes, block); err != nil {
				return nil, fmt.Errorf("Failed to parse block: %s", err)
			}
			return block, nil
		default:
			if r.logger.Core().Enabled(zap.DebugLevel) {
				r.logger.Debug("skipping unknown log line", zap.String("line", line))
			}
			continue
		}
	}

	r.logger.Info("lines channel has been closed")
	return nil, io.EOF
}

func (r *ConsoleReader) processData(reader io.Reader) error {
	scanner := r.buildScanner(reader)
	for scanner.Scan() {
		line := scanner.Text()
		r.lines <- line
	}

	if scanner.Err() == nil {
		close(r.lines)
		return io.EOF
	}

	return scanner.Err()
}

func (r *ConsoleReader) buildScanner(reader io.Reader) *bufio.Scanner {
	buf := make([]byte, 50*1024*1024)
	scanner := bufio.NewScanner(reader)
	scanner.Buffer(buf, 50*1024*1024)

	return scanner
}
