<?php

namespace App\Domain\Inventory;

use App\Domain\Item\Fragment\ElderGuardianFragment;
use App\Domain\Item\Fragment\MavenSplinter;
use App\Domain\Item\Fragment\ShaperGuardianFragment;
use App\Domain\Item\Fragment\UberElderElderFragment;
use App\Domain\Item\Fragment\UberElderShaperFragment;
use App\Domain\Item\Map\MavenWrit;
use App\Domain\Item\Set\ElderSet;
use App\Domain\Item\Set\ShaperSet;
use App\Domain\Item\Set\UberElderSet;

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
