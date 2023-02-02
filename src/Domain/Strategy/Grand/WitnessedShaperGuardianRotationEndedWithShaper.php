<?php

namespace App\Domain\Strategy\Grand;

use App\Domain\Inventory\Inventory;
use App\Domain\Strategy\GrandStrategy;
use App\Domain\Strategy\RunShaper;
use App\Domain\Strategy\RunShaperGuardianMap;
use App\Domain\Strategy\RunTheFormed;

class WitnessedShaperGuardianRotationEndedWithShaper extends GrandStrategy
{
    public function __invoke(Inventory $inventory): void
    {
        (new RunShaperGuardianMap())($inventory, 4);
        (new RunTheFormed())($inventory);
        (new RunShaper())($inventory);
    }
}
