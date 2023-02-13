<?php

namespace App\Presentation\Strategies;

use App\Domain\Inventory\Inventory;
use App\Infrastructure\Strategy\Runner;
use Symfony\Bundle\FrameworkBundle\Controller\AbstractController;
use Symfony\Component\HttpFoundation\JsonResponse;
use Symfony\Component\HttpFoundation\Response;
use Symfony\Component\Routing\Annotation\Route;

class Compose extends AbstractController
{
    #[Route("/strat/compose", name: "compose", methods: ["GET"])]
    public function index(Inventory $inventory, Runner $runner): Response
    {
        //TODO: Take strategies from real request
        $postedStrategies = [
            'run-shaper-guardian-map' => [
                'times' => 4,
                'strategies' => [
                    'simple-harvest' => [
                        'times' => 1,
                    ]
                ]
            ],
            'run-shaper' => [
                'times' => 1,
            ]
        ];

        $runner->handle($inventory, $postedStrategies);

        return new JsonResponse($inventory->getEndSummary());
    }
}
