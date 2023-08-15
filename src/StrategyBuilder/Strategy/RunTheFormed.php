<?php

namespace App\StrategyBuilder\Strategy;

use App\Item\Currency\OrbOfScouring;
use App\Item\Fragment\MavenSplinter;
use App\Item\Fragment\ShaperGuardianFragment;
use App\Item\Map\ShaperGuardianMap;
use App\Item\Map\TheFormed;

class RunTheFormed extends Strategy
{
    protected int $averageTime = 140; //2min 20s

    protected function setRequiredItems(): void
    {
        $this->requiredComponents = [
            [
                'item' => new TheFormed(),
                'quantity' => 1,
            ],
            [
                'item' => new OrbOfScouring(),
                'quantity' => 15,       // approx quant needed for rerolling 75%
            ],
        ];
    }

    public function yieldRewards(): mixed
    {
        return [
            [
                'item' => new MavenSplinter(),
                'quantity' => 6,
                'probability' => 100,
            ],
            [
                'item' => new ShaperGuardianFragment(),
                'quantity' => 1,
                'probability' => 100,
            ],
            [
                'item' => new ShaperGuardianMap(),
                'quantity' => 1,
                'probability' => 100,
            ],
        ];
    }
}
