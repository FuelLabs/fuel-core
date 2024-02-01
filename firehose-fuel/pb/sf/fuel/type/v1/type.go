package pbfuel

import (
	"encoding/hex"
	"time"

	firecore "github.com/streamingfast/firehose-core"
)

var _ firecore.Block = (*Block)(nil)

func (b *Block) GetFirehoseBlockID() string {
	return hex.EncodeToString(b.Id)
}

func (b *Block) GetFirehoseBlockNumber() uint64 {
	return uint64(b.Height)
}

func (b *Block) GetFirehoseBlockParentID() string {
	if b.PrevId == nil {
		return ""
	}

	return hex.EncodeToString(b.PrevId)
}

func (b *Block) GetFirehoseBlockParentNumber() uint64 {
	if b.Height == 0 {
		return 0
	}

	return uint64(b.Height - 1)
}

func (b *Block) GetFirehoseBlockTime() time.Time {
	return time.Unix(0, int64(b.Timestamp)).UTC()
}

func (b *Block) GetFirehoseBlockLIBNum() uint64 {
	return b.GetFirehoseBlockParentNumber()
}
