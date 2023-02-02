<?php

namespace App\Domain\Strategy\Grand;

use App\Domain\Inventory\Inventory;
use App\Domain\Strategy\GrandStrategy;
use App\Domain\Strategy\RunElderGuardianMap;
use App\Domain\Strategy\RunTheTwisted;

class WitnessedElderGuardianRotation extends GrandStrategy
{
    public function __invoke(Inventory $inventory): void
    {
        (new RunElderGuardianMap())($inventory, 4);
        (new RunTheTwisted())($inventory);
    }
}
