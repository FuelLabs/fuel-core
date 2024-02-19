package main

import (
	"fmt"
	"io"

	"github.com/streamingfast/bstream"
	pbfuel "github.com/FuelLabs/firehose-fuel/pb/sf/fuel/type/v1"
)

func printBlock(blk *bstream.Block, alsoPrintTransactions bool, out io.Writer) error {
	block := blk.ToProtocol().(*pbfuel.Block)

	if _, err := fmt.Fprintf(out, "Block #%d (%s) (prev: %s): %d transactions\n",
		block.Height,
		block.Id,
		block.PrevId[0:7],
		len(block.Transactions),
	); err != nil {
		return err
	}

	if alsoPrintTransactions {
		for i, _ := range block.Transactions {
			if _, err := fmt.Fprintf(out, "- Transaction %d\n", i); err != nil {
				return err
			}
		}
	}

	return nil
}
