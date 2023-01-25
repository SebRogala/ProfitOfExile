<?php

namespace App\Domain\Strategy;

use App\Domain\Item\Currency\BlueLifeforce;
use App\Domain\Item\Currency\PurpleLifeforce;
use App\Domain\Item\Currency\YellowLifeforce;

class RunSimpleHarvest extends Strategy
{
    protected function setRequiredItems(): void
    {
    }

    protected function yieldRewards(): mixed
    {
        return [
            [
                'item' => new YellowLifeforce(),
                'quantity' => 300,
                'probability' => 100,
            ],
            [
                'item' => new BlueLifeforce(),
                'quantity' => 100,
                'probability' => 100,
            ],
            [
                'item' => new PurpleLifeforce(),
                'quantity' => 100,
                'probability' => 100,
            ],
        ];
    }
}
