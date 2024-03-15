<?php

declare(strict_types=1);

namespace App\Helper;

class StringManipulation
{
    //public static function splitWords(string $str): string
    //{
    //    $splitNamespace = explode('\\', static::class);
    //
    //    $string = array_pop($splitNamespace);
    //    $parts = preg_split('/(?=[A-Z])/', $string);
    //
    //    return trim(implode(' ', $parts));
    //}

    public static function toKebabCase(string $str): string
    {
        return str_replace(' ', '-', strtolower($str));
    }

    public static function splitWords(string $str): string
    {
        $parts = preg_split('/(?=[A-Z])/', $str);

        return trim(implode(' ', $parts));
    }
}
