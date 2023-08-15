<?php

namespace App\StrategyBuilder\Strategy;

use App\Item\Fragment\ShaperGuardianFragment;
use App\Item\Map\ShaperGuardianMap;

class RunShaperGuardianMap extends Strategy
{
    protected int $averageTime = 60 * 2 + 30;

    public function yieldRewards(): mixed
    {
        return [
            [
                'item' => new ShaperGuardianFragment(),
                'quantity' => 1,
                'probability' => 100,
            ],
        ];
    }

    protected function setRequiredItems(): void
    {
        $this->requiredComponents = [
            [
                'item' => new ShaperGuardianMap(),
                'quantity' => 1,
            ],
        ];
    }
}
