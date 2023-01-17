<?php

namespace App\Domain\Item;

use App\Domain\Item\Currency\Currency;

class Value
{
    public function __construct(private float $quantity, private Currency $currency)
    {
    }
}
