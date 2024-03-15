<?php

namespace App\Domain\Trait;

use App\Helper\StringManipulation;

trait Name
{
    public function name(): string
    {
        $splitNamespace = explode('\\', static::class);
        $string = array_pop($splitNamespace);

        return StringManipulation::splitWords($string);
    }

    public function nameKey(): string
    {
        return StringManipulation::toKebabCase($this->name());
    }
}
