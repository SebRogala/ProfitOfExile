<?php

namespace App\Domain\Item;

abstract class Item
{
    public function name(): string
    {
        $splitNamespace = explode('\\', static::class);

        return array_pop($splitNamespace);
    }
}
