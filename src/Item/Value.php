<?php

namespace App\Item;

use App\Item\Currency\Currency;

class Value
{
    public function __construct(private float $quantity, private Currency $currency)
    {
    }
}
