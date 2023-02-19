<?php

namespace App\Presentation\Strategies;

use App\Infrastructure\Strategy\Factory;
use Symfony\Bundle\FrameworkBundle\Controller\AbstractController;
use Symfony\Component\HttpFoundation\JsonResponse;
use Symfony\Component\HttpFoundation\Response;
use Symfony\Component\Routing\Annotation\Route;

class GetStrategies extends AbstractController
{
    #[Route("/strat/get-strategies", name: "get-strategies", methods: ["GET"])]
    public function index(): Response
    {
        $ret = [];

        foreach (Factory::STRATEGIES as $stratName => $x) {
            $strategy = Factory::create($stratName);

            $ret[] = [
                'key' => $stratName,
                'name' => $strategy->name(),
                'averageTime' => $strategy->getAverageTime(),
                'probability' => $strategy->getOccurrenceProbability(),
            ];
        }

        return new JsonResponse($ret);
    }
}
