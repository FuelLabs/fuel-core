package codec

import (
	"bufio"
	"encoding/base64"
	"fmt"
	pbfuel "github.com/FuelLabs/firehose-fuel/pb/sf/fuel/type/v1"
	"github.com/golang/protobuf/proto"
	"github.com/streamingfast/bstream"
	firecore "github.com/streamingfast/firehose-core"
	"github.com/streamingfast/logging"
	"github.com/streamingfast/node-manager/mindreader"
	pbbstream "github.com/streamingfast/pbgo/sf/bstream/v1"
	"go.uber.org/zap"
	"io"
	"strconv"
	"strings"
)

// ConsoleReader is what reads the `fuel-core` output directly
type ConsoleReader struct {
	lines        chan string
	blockEncoder firecore.BlockEncoder
	close        func()

	done        chan interface{}
	activeBlock *pbfuel.Block

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
	fmt.Printf("ReadBlock Block: %+v\n", block)
	return r.blockEncoder.Encode(block)
	//return BlockFromProto(block)
}

const (
	LogPrefix     = "FIRE "
	LogBlockBegin = "BLOCK_BEGIN"
	LogBeginTrx   = "BEGIN_TRX"
	LogBlockEnd   = "BLOCK_END"
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
		//

		// Order the case from most occurring line prefix to least occurring
		switch args[0] {
		case LogBlockBegin:
			err = r.readBlockBegin(args[1:])

		case LogBeginTrx:
			err = r.readTransaction(args[1:])

		case LogBlockEnd:
			//This end the execution of the reading loop as we have a full block here
			//block, err := r.readBlockEnd(args[1:])
			block, err := r.readBlockEnd(args)
			if err != nil {
				return nil, lineError(line, err)
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

func (r *ConsoleReader) readBlockBegin(params []string) error {
	if err := validateChunk(params, 1); err != nil {
		return fmt.Errorf("invalid BLOCK_BEGIN line: %w", err)
	}

	height, err := strconv.ParseUint(params[0], 10, 64)
	if err != nil {
		return fmt.Errorf(`invalid BLOCK_BEGIN "height" param: %w`, err)
	}

	if r.activeBlock != nil {
		r.logger.Info("received BLOCK_BEGIN while one is already active, resetting active block and starting over",
			zap.Uint64("previous_active_block_height", uint64(r.activeBlock.Height)),
			zap.Uint64("new_active_block_height", height),
		)
	}

	r.activeBlock = &pbfuel.Block{
		Height: uint32(height),
	}

	return nil
}

// Format:
// FIRE TRX <sf.aptos.type.v1.Transaction>
func (r *ConsoleReader) readTransaction(params []string) error {
	if err := validateChunk(params, 1); err != nil {
		return fmt.Errorf("invalid log line length: %w", err)
	}

	if r.activeBlock == nil {
		return fmt.Errorf("no active block in progress when reading TRX")
	}

	out, err := base64.StdEncoding.DecodeString(params[0])
	if err != nil {
		return fmt.Errorf("read trx in block %d: invalid base64 value: %w", r.activeBlock.Height, err)
	}

	transaction := &pbfuel.Transaction{}
	if err := proto.Unmarshal(out, transaction); err != nil {
		return fmt.Errorf("read trx in block %d: invalid proto: %w", r.activeBlock.Height, err)
	}

	r.activeBlock.Transactions = append(r.activeBlock.Transactions, transaction)

	return nil
}

// func (r *ConsoleReader) readBlockEnd(params []string) (*pbfuel.Block, error) {
func (r *ConsoleReader) readBlockEnd(params []string) (*pbfuel.Block, error) {
	// Todo: Validations - check height again

	height, err := strconv.ParseUint(params[1:][0], 10, 64)

	if err != nil {
		return nil, fmt.Errorf(`invalid BLOCK_END "height" param: %w`, err)
	}

	if r.activeBlock == nil {
		return nil, fmt.Errorf("no active block in progress when reading BLOCK_END")
	}

	if r.activeBlock.GetFirehoseBlockNumber() != height {
		return nil, fmt.Errorf("active block's height %d does not match BLOCK_END received height %d", r.activeBlock.Height, height)
	}

	// Todo Change when fuel node comes
	r.activeBlock.Id = []byte(params[2:][0])
	r.activeBlock.PrevId = []byte(params[3:][0])
	//r.activeBlock.Timestamp =

	r.logger.Debug("console reader node block",
		zap.String("id", r.activeBlock.GetFirehoseBlockID()),
		zap.Uint64("height", r.activeBlock.GetFirehoseBlockNumber()),
		zap.Time("timestamp", r.activeBlock.GetFirehoseBlockTime()),
	)

	block := r.activeBlock
	r.resetActiveBlock()

	return block, nil
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

func (r *ConsoleReader) resetActiveBlock() {
	r.activeBlock = nil
}

func validateChunk(params []string, count int) error {
	if len(params) != count {
		return fmt.Errorf("%d fields required but found %d", count, len(params))
	}
	return nil
}

func lineError(line string, source error) error {
	return fmt.Errorf("%w (on line %q)", source, line)
}

func BlockFromProto(b *pbfuel.Block) (*bstream.Block, error) {
	content, err := proto.Marshal(b)
	if err != nil {
		return nil, fmt.Errorf("unable to marshal to binary form: %s", err)
	}

	block := &bstream.Block{
		Id:             b.GetFirehoseBlockID(),
		Number:         b.GetFirehoseBlockNumber(),
		PreviousId:     b.GetFirehoseBlockParentID(),
		Timestamp:      b.GetFirehoseBlockTime(),
		LibNum:         b.GetFirehoseBlockLIBNum(),
		PayloadKind:    pbbstream.Protocol_UNKNOWN,
		PayloadVersion: 1,
	}

	println(block.Id)

	return bstream.GetBlockPayloadSetter(block, content)
}
