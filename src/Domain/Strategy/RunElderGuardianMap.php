<?php

namespace App\Domain\Strategy;

use App\Domain\Item\Fragment\ElderGuardianFragment;
use App\Domain\Item\Map\ElderGuardianMap;

class RunElderGuardianMap extends Strategy
{
    protected int $averageTime = 60 * 2 + 30;

    protected function setRequiredItems(): void
    {
        $this->requiredComponents = [
            [
                'item' => new ElderGuardianMap(),
                'quantity' => 1,
            ],
        ];
    }

    public function yieldRewards(): mixed
    {
        return [
            [
                'item' => new ElderGuardianFragment(),
                'quantity' => 1,
                'probability' => 100,
            ],
        ];
    }
}
