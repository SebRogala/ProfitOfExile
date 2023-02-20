<?php

namespace App\Presentation\Strategy;

use App\Domain\Inventory\Inventory;
use App\Infrastructure\Strategy\Runner;
use Symfony\Bundle\FrameworkBundle\Controller\AbstractController;
use Symfony\Component\HttpFoundation\JsonResponse;
use Symfony\Component\HttpFoundation\Request;
use Symfony\Component\HttpFoundation\Response;
use Symfony\Component\Routing\Annotation\Route;

class Compose extends AbstractController
{
    #[Route("/strategy/compose", name: "compose", methods: ["POST"])]
    public function index(Request $request, Inventory $inventory, Runner $runner): Response
    {
        $runner->handle($inventory, $request->toArray());

        return new JsonResponse($inventory->getEndSummary());
    }
}
