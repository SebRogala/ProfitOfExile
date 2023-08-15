<?php

namespace App\StrategyBuilder\Strategy;

use App\Item\Currency\BlueLifeforce;
use App\Item\Currency\PurpleLifeforce;
use App\Item\Currency\YellowLifeforce;

class RunSimpleHarvest extends Strategy
{
    protected int $averageTime = 120;

    protected int $occurrenceProbability = 90;

    protected function setRequiredItems(): void
    {
    }

    public function yieldRewards(): mixed
    {
        return [
            [
                'item' => new YellowLifeforce(),
                'quantity' => 250,
                'probability' => 100,
            ],
            [
                'item' => new BlueLifeforce(),
                'quantity' => 150,
                'probability' => 100,
            ],
            [
                'item' => new PurpleLifeforce(),
                'quantity' => 150,
                'probability' => 100,
            ],
        ];
    }
}
