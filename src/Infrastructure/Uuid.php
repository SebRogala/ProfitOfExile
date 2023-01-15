<?php

namespace App\Infrastructure;

class Uuid
{
    public static function new():string
    {
        return \Ramsey\Uuid\Uuid::uuid4()->toString();
    }
}
