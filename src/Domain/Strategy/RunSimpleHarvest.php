<?php

namespace App\Domain\Strategy;

use App\Domain\Item\Currency\BlueLifeforce;
use App\Domain\Item\Currency\PurpleLifeforce;
use App\Domain\Item\Currency\YellowLifeforce;

class RunSimpleHarvest extends Strategy
{
    protected int $averageTime = 120;

    protected int $occurrenceProbability = 80;

    protected function setRequiredItems(): void
    {
    }

    public function yieldRewards(): mixed
    {
        return [
            [
                'item' => new YellowLifeforce(),
                'quantity' => 200,
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
