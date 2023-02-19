<?php

namespace App\Domain\Strategy;

use App\Domain\Item\Currency\OrbOfScouring;
use App\Domain\Item\Fragment\MavenSplinter;
use App\Domain\Item\Fragment\ShaperGuardianFragment;
use App\Domain\Item\Map\ShaperGuardianMap;
use App\Domain\Item\Map\TheFormed;

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
