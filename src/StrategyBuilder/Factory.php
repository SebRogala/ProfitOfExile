<?php

namespace App\StrategyBuilder;

use App\Item\Factory as ItemFactory;
use App\StrategyBuilder\Strategy\Custom;
use App\StrategyBuilder\Strategy\Exception\StrategyNotFoundException;
use App\StrategyBuilder\Strategy\RunShaper;
use App\StrategyBuilder\Strategy\RunShaperGuardianMap;
use App\StrategyBuilder\Strategy\RunSimpleHarvest;
use App\StrategyBuilder\Strategy\RunTheFormed;
use App\StrategyBuilder\Strategy\Strategy;
use App\StrategyBuilder\Strategy\Wrapper;

class Factory
{
    public const STRATEGIES = [
        'wrapper' => Wrapper::class,
        'custom' => Custom::class,
        'run-shaper' => RunShaper::class,
        'run-shaper-guardian-map' => RunShaperGuardianMap::class,
        'run-simple-harvest' => RunSimpleHarvest::class,
        'run-the-formed' => RunTheFormed::class,
    ];

    public function __construct(private ItemFactory $factory)
    {
    }

    public function create(string $name): Strategy
    {
        if (!array_key_exists($name, self::STRATEGIES)) {
            throw new StrategyNotFoundException();
        }

        $class = self::STRATEGIES[$name];

        return match ($name) {
            'custom' => new $class($this->factory),
            default => new $class(),
        };
    }
}
