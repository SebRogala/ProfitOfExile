<?php

namespace App\Domain\Inventory;

use App\Item\Fragment\ElderGuardianFragment;
use App\Item\Fragment\MavenSplinter;
use App\Item\Fragment\ShaperGuardianFragment;
use App\Item\Fragment\UberElderElderFragment;
use App\Item\Fragment\UberElderShaperFragment;
use App\Item\Map\MavenWrit;
use App\Item\Set\ElderSet;
use App\Item\Set\ShaperSet;
use App\Item\Set\UberElderSet;

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

        $elderGuardianFragment = new ElderGuardianFragment();
        while ($inventory->hasItems($elderGuardianFragment, 4)) {
            $inventory->removeItems($elderGuardianFragment, 4);
            $inventory->add(new ElderSet());
        }

        $uberElderShaperFragment = new UberElderShaperFragment();
        $uberElderElderFragment = new UberElderElderFragment();
        while ($inventory->hasItems($uberElderShaperFragment, 2) && $inventory->hasItems($uberElderElderFragment, 2)) {
            $inventory->removeItems($uberElderShaperFragment, 2);
            $inventory->removeItems($uberElderElderFragment, 2);
            $inventory->add(new UberElderSet());
        }

        return $inventory->getItems();
    }
}
