<?php

namespace App\Presentation\Strategy;

use App\StrategyBuilder\Factory;
use Symfony\Bundle\FrameworkBundle\Controller\AbstractController;
use Symfony\Component\HttpFoundation\JsonResponse;
use Symfony\Component\HttpFoundation\Response;
use Symfony\Component\Routing\Annotation\Route;

class GetAll extends AbstractController
{
    #[Route("/strategy/get-all", name: "get-strategies", methods: ["GET"])]
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
