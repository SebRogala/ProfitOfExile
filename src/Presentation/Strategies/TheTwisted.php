<?php

namespace App\Presentation\Strategies;

use App\Domain\Inventory\Inventory;
use App\Domain\Strategy\Grand\WitnessedElderGuardianRotation;
use Symfony\Bundle\FrameworkBundle\Controller\AbstractController;
use Symfony\Component\HttpFoundation\JsonResponse;
use Symfony\Component\HttpFoundation\Response;
use Symfony\Component\Routing\Annotation\Route;

class TheTwisted extends AbstractController
{
    #[Route("/strat/the-twisted", name: "the-twisted", methods: ["GET"])]
    public function index(Inventory $inventory): Response
    {
        (new WitnessedElderGuardianRotation())($inventory);

        return new JsonResponse($inventory->getEndSummary());
    }
}
