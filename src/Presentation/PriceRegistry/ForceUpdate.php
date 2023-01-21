<?php

namespace App\Presentation\PriceRegistry;

use App\Application\Command\PriceRegistry\UpdateRegistry;
use App\Application\CommandBus;
use Symfony\Bundle\FrameworkBundle\Controller\AbstractController;
use Symfony\Component\HttpFoundation\Response;
use Symfony\Component\Routing\Annotation\Route;

class ForceUpdate extends AbstractController
{
    #[Route("/price-registry/force-update",    name: "price_registry_force_update",    methods: ["GET"])]
    public function execute(CommandBus $commandBus): Response
    {
        $command = new UpdateRegistry(true);
        $commandBus->handle($command);

        return new Response();
    }
}
