<?php

namespace App\Domain\Strategy;

use App\Domain\Inventory\Inventory;

abstract class GrandStrategy
{
    abstract public function __invoke(Inventory $inventory): void;
}
