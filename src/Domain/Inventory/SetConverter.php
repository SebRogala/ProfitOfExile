<?php

namespace App\Domain\Inventory;

use App\Domain\Item\Fragment\MavenSplinter;
use App\Domain\Item\Fragment\ShaperGuardianFragment;
use App\Domain\Item\Map\MavenWrit;
use App\Domain\Item\Set\ShaperSet;

class SetConverter
{
    public function convertToSets(Inventory $inventory): array
    {
        $shaperGuardianFragment = new ShaperGuardianFragment();
        while ($inventory->hasItems($shaperGuardianFragment, 4)) {
            $inventory->removeItems($shaperGuardianFragment, 4);
            $inventory->add(new ShaperSet());
        }

        $mavenSplinter = new MavenSplinter();
        while ($inventory->hasItems($mavenSplinter, 10)) {
            $inventory->removeItems($mavenSplinter, 10);
            $inventory->add(new MavenWrit());
        }

        return $inventory->getItems();
    }
}
