package pbfuel

import (
	"encoding/hex"
	"time"
)

func (b *Block) GetFirehoseBlockID() string {
	return hex.EncodeToString(b.Id)
}

func (b *Block) GetFirehoseBlockNumber() uint64 {
	return uint64(b.Height)
}

func (b *Block) GetFirehoseBlockParentNumber() uint64 {
	// TODO: This needs to be adapted for your own chain rules!
	return b.GetFirehoseBlockNumber() - 1
}

func (b *Block) GetFirehoseBlockParentID() string {
	return hex.EncodeToString(b.PrevId)
}

func (b *Block) GetFirehoseBlockTime() time.Time {
	return time.Unix(0, int64(b.Timestamp)).UTC()
}

func (b *Block) GetFirehoseBlockVersion() int32 {
	// TODO: This needs to be adapted for your own version used in pbbstream
	return 1
}

func (b *Block) GetFirehoseBlockLIBNum() uint64 {
	if b.Height == 0 {
		return 0
	}

	// TODO: This needs to be adapted for your own chain rules!
	return b.GetFirehoseBlockNumber() - 1
}
