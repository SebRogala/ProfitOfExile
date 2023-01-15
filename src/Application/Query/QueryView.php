<?php

namespace App\Application\Query;

interface QueryView
{
    public function toArray(): array;

    public static function fromArray(array $data): self;
}
