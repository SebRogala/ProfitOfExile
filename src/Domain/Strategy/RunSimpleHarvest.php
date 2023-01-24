<?php

namespace App\Domain\Strategy;

use App\Domain\Item\Currency\DivineOrb;

class RunSimpleHarvest extends Strategy
{
    protected function yieldRewards(): mixed
    {
        return [
            [
                'item' => new DivineOrb(),
                'quantity' => 1,
                'probability' => 100,
            ],
        ];
    }

    protected function setRequiredItems(): void
    {
        // TODO: Implement setRequiredItems() method.
    }
}
