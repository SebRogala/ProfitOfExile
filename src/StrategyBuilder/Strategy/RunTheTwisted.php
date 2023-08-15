<?php

namespace App\StrategyBuilder\Strategy;

use App\Item\Currency\OrbOfScouring;
use App\Item\Fragment\ElderGuardianFragment;
use App\Item\Fragment\MavenSplinter;
use App\Item\Map\TheTwisted;

class RunTheTwisted extends Strategy
{
    protected int $averageTime = 140; //2min 20s

    protected function setRequiredItems(): void
    {
        $this->requiredComponents = [
            [
                'item' => new TheTwisted(),
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
                'item' => new ElderGuardianFragment(),
                'quantity' => 2,
                'probability' => 100,
            ],
        ];
    }
}
