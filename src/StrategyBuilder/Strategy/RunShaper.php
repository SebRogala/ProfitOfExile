<?php

namespace App\StrategyBuilder\Strategy;

use App\Item\Fragment\UberElderShaperFragment;
use App\Item\Set\ShaperSet;

class RunShaper extends Strategy
{
    protected int $averageTime = 60 * 8;

    protected function setRequiredItems(): void
    {
        $this->requiredComponents = [
            [
                'item' => new ShaperSet(),
                'quantity' => 1,
            ],
        ];
    }

    public function yieldRewards(): mixed
    {
        return [
            [
                'item' => new UberElderShaperFragment(),
                'quantity' => 1,
                'probability' => 100,
            ],
        ];
    }
}
