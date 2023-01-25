<?php

namespace App\Domain\Strategy;

use App\Domain\Inventory\Inventory;
use App\Domain\Item\Fragment\ShaperGuardianFragment;
use App\Domain\Item\Map\ShaperGuardianMap;

class WitnessedShaperGuardianMaps extends Strategy
{
    public function yieldRewards(): mixed
    {
        return [
            [
                'item' => new ShaperGuardianFragment(),
                'quantity' => 4,
                'probability' => 100,
            ],
        ];
    }

    protected function setRequiredItems(): void
    {
        $this->requiredComponents = [
            [
                'item' => new ShaperGuardianMap(),
                'quantity' => 4,
            ],
        ];
    }

    protected function setAverageTime(): void
    {
        // TODO: Implement setAverageTime() method.
    }

}
