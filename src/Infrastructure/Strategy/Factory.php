<?php

namespace App\Infrastructure\Strategy;

use App\Domain\Strategy\Exception\StrategyNotFoundException;
use App\Domain\Strategy\RunSimpleHarvest;
use App\Domain\Strategy\RunTheFormed;
use App\Domain\Strategy\Strategy;
use App\Domain\Strategy\RunShaper;
use App\Domain\Strategy\RunShaperGuardianMap;

class Factory
{
    public const STRATEGIES = [
        'run-shaper' => RunShaper::class,
        'run-shaper-guardian-map' => RunShaperGuardianMap::class,
        'run-simple-harvest' => RunSimpleHarvest::class,
        'run-the-formed' => RunTheFormed::class,
    ];

    public static function create(string $name): Strategy
    {
        if (!array_key_exists($name, self::STRATEGIES)) {
            throw new StrategyNotFoundException();
        }

        $class = self::STRATEGIES[$name];

        return new $class();
    }
}
