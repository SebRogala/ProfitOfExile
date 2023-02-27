<?php

namespace App\Domain\Trait;

trait Name
{
    public function name(): string
    {
        $splitNamespace = explode('\\', static::class);

        $string = array_pop($splitNamespace);
        $parts = preg_split('/(?=[A-Z])/', $string);

        return trim(implode(' ', $parts));
    }
}
