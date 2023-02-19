<?php

namespace App\Presentation\Strategy;

use App\Domain\Inventory\Inventory;
use App\Infrastructure\Strategy\Runner;
use Symfony\Bundle\FrameworkBundle\Controller\AbstractController;
use Symfony\Component\HttpFoundation\JsonResponse;
use Symfony\Component\HttpFoundation\Response;
use Symfony\Component\Routing\Annotation\Route;

class Compose extends AbstractController
{
    #[Route("/strategy/compose", name: "compose", methods: ["GET"])]
    public function index(Inventory $inventory, Runner $runner): Response
    {
        //TODO: Take strategies from real request
        $postedStrategies = [
            'wrapper' => [
                'series' => 2,
                'strategies' => [
                    'run-shaper-guardian-map' => [
                        'series' => 4,
                    ],
                    'run-the-formed' => [],
                    'run-shaper' => [],
                ],
            ],
        ];

        $runner->handle($inventory, $postedStrategies);

        return new JsonResponse($inventory->getEndSummary());
    }
}
