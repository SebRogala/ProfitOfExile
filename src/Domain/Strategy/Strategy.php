<?php

namespace App\Domain\Strategy;

abstract class Strategy
{
    protected array $requiredComponents = [];

    abstract public function getRewards(): mixed;
}
