<?php

namespace App\Domain\Item\Set;

use App\Domain\Item\Value;

abstract class Set
{
    abstract public function getElementalValue(): Value;
    abstract public function getBulkValue(): Value;
    abstract public function getRewards();
}
